
/// Data quality pipeline subcommands (powered by alimentar).
///
/// Thin CLI wrappers around alimentar's data utilities.
#[derive(Subcommand, Debug)]
pub enum DataCommands {
    /// Prepare the TweetEval abortion stance benchmark as aprender JSONL
    TweetEvalStance {
        /// Output directory for JSONL splits and benchmark-manifest.json
        #[arg(short, long, value_name = "DIR")]
        output: PathBuf,
        /// Split layout: canonical (train/validation/test) or setfit (train/test)
        #[arg(long, value_enum, default_value_t = TweetEvalStanceProfile::Canonical)]
        profile: TweetEvalStanceProfile,
        /// Existing canonical TweetEval abortion directory (disables download)
        #[arg(long, value_name = "DIR")]
        source: Option<PathBuf>,
        /// Pinned TweetEval git revision used for provenance and downloads
        #[arg(long, default_value = crate::commands::data_tweeteval::CANONICAL_REVISION)]
        revision: String,
        /// Replace benchmark files already present in the output directory
        #[arg(long)]
        force: bool,
    },
    /// Audit a JSONL classification dataset for quality issues
    Audit {
        /// Path to JSONL data file
        #[arg(value_name = "FILE")]
        file: PathBuf,
        /// Number of output classes (for label range validation)
        #[arg(long, default_value = "5")]
        num_classes: usize,
        /// Input text column name
        #[arg(long, default_value = "input")]
        input_column: String,
        /// Label column name
        #[arg(long, default_value = "label")]
        label_column: String,
        /// Preamble prefix to detect (e.g., "#!/")
        #[arg(long, default_value = "#!/")]
        preamble_prefix: Option<String>,
    },
    /// Stratified train/val/test split preserving class proportions
    Split {
        /// Path to JSONL data file
        #[arg(value_name = "FILE")]
        file: PathBuf,
        /// Training set fraction
        #[arg(long, default_value = "0.8")]
        train: f64,
        /// Validation set fraction
        #[arg(long, default_value = "0.1")]
        val: f64,
        /// Test set fraction
        #[arg(long, default_value = "0.1")]
        test: f64,
        /// Label column name for stratification
        #[arg(long, default_value = "label")]
        label_column: String,
        /// Random seed for deterministic split
        #[arg(long, default_value = "42")]
        seed: u64,
        /// Output directory for split files
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Check training data for benchmark contamination via n-gram overlap
    Decontaminate {
        /// Path to training JSONL data file
        #[arg(value_name = "FILE")]
        file: PathBuf,
        /// Reference benchmark JSONL files to check against
        #[arg(long, required = true, num_args = 1..)]
        reference: Vec<PathBuf>,
        /// N-gram size for overlap detection
        #[arg(long, default_value = "10")]
        ngram: usize,
        /// Overlap threshold (0.0-1.0) above which a sample is flagged
        #[arg(long, default_value = "0.5")]
        threshold: f64,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Resample dataset to address class imbalance
    Balance {
        /// Path to JSONL data file
        #[arg(value_name = "FILE")]
        file: PathBuf,
        /// Rebalancing strategy: oversample, undersample, sqrt-inverse
        #[arg(long, default_value = "oversample")]
        strategy: String,
        /// Label column name
        #[arg(long, default_value = "label")]
        label_column: String,
        /// Number of classes (for sqrt-inverse weight computation)
        #[arg(long)]
        num_classes: Option<usize>,
        /// Random seed
        #[arg(long, default_value = "42")]
        seed: u64,
        /// Output file path (required for oversample/undersample)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

/// Supported layouts for the TweetEval abortion stance benchmark.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TweetEvalStanceProfile {
    /// Original TweetEval train/validation/test splits (recommended).
    Canonical,
    /// SetFit wrapper layout: train plus validation+test merged as test.
    Setfit,
}
