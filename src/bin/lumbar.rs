use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use lumbar::{
    Access, Associativity, CacheConfigBuilder, Hierarchy, LevelStats, ReplacementPolicy, Simulator,
    belady_optimal, patterns,
};

/// Simulate a memory access pattern against a single cache level.
#[derive(Parser)]
#[command(name = "lumbar", version, about)]
struct Cli {
    /// Cache size in bytes
    #[arg(long, default_value_t = 32 * 1024)]
    cache_size: usize,
    /// Line size in bytes
    #[arg(long, default_value_t = 64)]
    line_size: usize,
    /// Ways per set (1 = direct-mapped, 0 = fully associative)
    #[arg(long, default_value_t = 8)]
    ways: usize,
    /// Replacement policy (ignored if --belady is set)
    #[arg(long, value_enum, default_value_t = PolicyArg::Lru)]
    policy: PolicyArg,
    /// Compare against Belady's optimal offline policy instead of --policy
    #[arg(long)]
    belady: bool,
    /// Attach a victim cache with this many lines
    #[arg(long, default_value_t = 0)]
    victim_lines: usize,
    /// Cache latency in cycles, for the reported AMAT
    #[arg(long, default_value_t = 4)]
    cache_latency: u32,
    /// Backing memory latency in cycles, for the reported AMAT
    #[arg(long, default_value_t = 200)]
    mem_latency: u32,

    #[command(subcommand)]
    pattern: Pattern,
}

#[derive(Clone, Copy, ValueEnum)]
enum PolicyArg {
    Lru,
    Fifo,
    Random,
}

impl From<PolicyArg> for ReplacementPolicy {
    fn from(policy: PolicyArg) -> Self {
        match policy {
            PolicyArg::Lru => ReplacementPolicy::Lru,
            PolicyArg::Fifo => ReplacementPolicy::Fifo,
            PolicyArg::Random => ReplacementPolicy::Random,
        }
    }
}

#[derive(Subcommand)]
enum Pattern {
    /// Sequential row-major traversal
    RowMajor {
        rows: usize,
        cols: usize,
        #[arg(long, default_value_t = 4)]
        elem_size: usize,
    },
    /// Column-major traversal of a row-major array
    ColMajor {
        rows: usize,
        cols: usize,
        #[arg(long, default_value_t = 4)]
        elem_size: usize,
    },
    /// Out-of-place matrix transpose
    Transpose {
        rows: usize,
        cols: usize,
        #[arg(long, default_value_t = 4)]
        elem_size: usize,
    },
    /// Naive triple-nested-loop matrix multiply
    MatmulNaive {
        m: usize,
        n: usize,
        k: usize,
        #[arg(long, default_value_t = 4)]
        elem_size: usize,
    },
    /// Blocked/tiled matrix multiply
    MatmulBlocked {
        m: usize,
        n: usize,
        k: usize,
        block: usize,
        #[arg(long, default_value_t = 4)]
        elem_size: usize,
    },
    /// 2D von Neumann stencil
    Stencil {
        rows: usize,
        cols: usize,
        radius: usize,
        iterations: usize,
        #[arg(long, default_value_t = 4)]
        elem_size: usize,
    },
    /// Read a trace from a file: one access per line, "r <addr> <size>" or
    /// "w <addr> <size>"; blank lines and lines starting with '#' are skipped
    Trace { path: PathBuf },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let trace: Vec<Access> = match &cli.pattern {
        Pattern::RowMajor {
            rows,
            cols,
            elem_size,
        } => patterns::row_major(*rows, *cols, *elem_size).collect(),
        Pattern::ColMajor {
            rows,
            cols,
            elem_size,
        } => patterns::col_major(*rows, *cols, *elem_size).collect(),
        Pattern::Transpose {
            rows,
            cols,
            elem_size,
        } => patterns::transpose(*rows, *cols, *elem_size).collect(),
        Pattern::MatmulNaive { m, n, k, elem_size } => {
            patterns::matmul_naive(*m, *n, *k, *elem_size).collect()
        }
        Pattern::MatmulBlocked {
            m,
            n,
            k,
            block,
            elem_size,
        } => patterns::matmul_blocked(*m, *n, *k, *block, *elem_size).collect(),
        Pattern::Stencil {
            rows,
            cols,
            radius,
            iterations,
            elem_size,
        } => patterns::stencil_2d(*rows, *cols, *radius, *iterations, *elem_size).collect(),
        Pattern::Trace { path } => read_trace_file(path)?,
    };

    let associativity = match cli.ways {
        0 => Associativity::FullyAssociative,
        1 => Associativity::DirectMapped,
        n => Associativity::SetAssociative(n.try_into().expect("checked nonzero above")),
    };

    let mut builder = CacheConfigBuilder::new(cli.cache_size, cli.line_size, associativity)
        .replacement_policy(cli.policy.into())
        .latency_cycles(cli.cache_latency);
    if cli.victim_lines > 0 {
        builder = builder.victim_cache(cli.victim_lines);
    }
    let cache_config = builder.build().context("invalid cache configuration")?;

    if cli.belady {
        let stats = belady_optimal(&cache_config, &trace);
        print_stats("belady (optimal)", &stats, None);
    } else {
        let hierarchy = Hierarchy::builder()
            .add_cache("L1", cache_config)
            .backing("mem", cli.mem_latency);
        let mut sim = Simulator::new(hierarchy);
        let result = sim.run(trace);
        print_stats("L1", &result.per_level[0], Some(result.amat_cycles));
    }

    Ok(())
}

fn print_stats(name: &str, stats: &LevelStats, amat: Option<f64>) {
    println!(
        "{name}: {} accesses, {} hits, {} misses, hit rate {:.1}%",
        stats.accesses(),
        stats.hits,
        stats.misses,
        stats.hit_rate() * 100.0
    );
    if let Some(amat) = amat {
        println!("AMAT: {amat:.2} cycles");
    }
}

fn read_trace_file(path: &PathBuf) -> Result<Vec<Access>> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(parse_trace_line)
        .collect()
}

fn parse_trace_line(line: &str) -> Result<Access> {
    let mut parts = line.split_whitespace();
    let kind = parts.next().context("missing access kind (r/w)")?;
    let addr: u64 = parts
        .next()
        .context("missing address")?
        .parse()
        .context("invalid address")?;
    let size: u32 = parts
        .next()
        .context("missing size")?
        .parse()
        .context("invalid size")?;
    match kind {
        "r" | "R" => Ok(Access::read(addr, size)),
        "w" | "W" => Ok(Access::write(addr, size)),
        other => bail!("unknown access kind '{other}', expected 'r' or 'w'"),
    }
}
