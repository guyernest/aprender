
/// Native SetFit classifier TRAINING (`setfit-apr-v1` artifacts).
///
/// # This namespace is training-only, and that is a locked decision (D-06)
///
/// `predict`, `eval` and `inspect` for a SetFit artifact are the GENERIC `apr`
/// commands — they take a model path like every other model path, and they work on
/// `setfit-apr-v1` because the artifact is an APR. Adding `apr setfit predict` here
/// would give a user two spellings of one operation and would make the SetFit
/// artifact look like a format that needs its own tooling, which is the opposite of
/// what shipping it as APR is for.
///
/// Training gets a namespace of its own because it is the one operation with no
/// generic form: it consumes Phase 2's prepared dataset directory and selection
/// manifest, which no other `apr` command knows about.
#[derive(Subcommand, Debug)]
pub enum SetfitCommands {
    /// Train a SetFit classifier from Phase 2 artifacts and write a verified APR
    ///
    /// Configuration is FILE-FIRST: the twelve training knobs come from --config,
    /// and only --seed and --device may override it. That is what makes a run
    /// reproducible from the reported configuration alone — a knob that exists only
    /// as a flag has no place in the artifact's recorded provenance.
    ///
    /// The dataset directory and the selection manifest are consumed exactly as
    /// `apr data tweet-eval-stance` and `apr data select` wrote them. This command
    /// introduces no new on-disk format.
    Train {
        /// Training configuration: `.toml` or `.json`, carrying all twelve knobs
        ///
        /// Deserialization IS validation — the file is parsed straight through the
        /// library's single validating constructor, so an unknown key or an invalid
        /// value is refused BEFORE anything is read from disk. There is no partial
        /// config and no per-knob default: an absent `root_seed` is an error, not a
        /// silently chosen number nobody would be able to reproduce.
        #[arg(long, value_name = "FILE")]
        config: PathBuf,

        /// Attested benchmark directory, as written by `apr data tweet-eval-stance`
        ///
        /// The canonical train/validation/test JSONL plus benchmark-manifest.json.
        /// A compatibility-profile, mixed, stale or forged directory is refused at
        /// the attested boundary before a single row is read into training.
        #[arg(long, value_name = "DIR")]
        data: PathBuf,

        /// The selection-manifest.json written by `apr data select`
        ///
        /// Strictly replayed against --data before training: the manifest names the
        /// rows, and a manifest whose digest, provenance or ordered list does not
        /// survive replay against THIS dataset is refused.
        #[arg(long, value_name = "FILE")]
        selection: PathBuf,

        /// Pinned all-MiniLM-L6-v2 checkout: tokenizer.json plus the encoder weights
        ///
        /// OFFLINE PREREQUISITE. This command never downloads. Obtain the pinned
        /// revision separately (for example `batuta hf pull`) and point --model-dir
        /// at the directory. The tokenizer bytes must hash to the pinned digest, so
        /// a near-miss checkout is refused rather than silently producing a model
        /// whose tokenizer and encoder disagree.
        #[arg(long = "model-dir", value_name = "DIR")]
        model_dir: PathBuf,

        /// Where to write the `setfit-apr-v1` artifact
        #[arg(short, long, value_name = "FILE")]
        output: PathBuf,

        /// Override the config file's `root_seed`
        ///
        /// The override is merged through the library's public validated door and
        /// the WHOLE merged configuration is revalidated, so an override cannot
        /// smuggle past a check the file had to pass. The merged value is what the
        /// run records; the file's value is not preserved anywhere.
        #[arg(long, value_name = "SEED")]
        seed: Option<u64>,

        /// Override the config file's `device` (`cpu`, `cuda`, `cuda:N`, `auto`)
        ///
        /// An explicitly requested device that this host cannot provide is a HARD
        /// failure with a nonzero exit code. There is no silent fallback to CPU:
        /// a benchmark number produced on a device nobody asked for is worse than
        /// no number at all.
        #[arg(long, value_name = "SPEC")]
        device: Option<String>,

        /// Replace an existing --output file
        #[arg(long)]
        force: bool,

        /// Validate the request and the Phase 2 inputs, then stop
        ///
        /// Reports the MERGED resolved configuration and exits without loading
        /// --model-dir, without training and without writing anything. The encoder
        /// load is deliberately outside the dry run: it is a multi-hundred-megabyte
        /// read, and a pre-flight that costs as much as the thing it precedes is not
        /// a pre-flight.
        #[arg(long = "dry-run")]
        dry_run: bool,
    },
}
