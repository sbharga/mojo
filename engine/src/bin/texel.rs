use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use cozy_chess::Board;
use mojo_engine::tuning::{
    PARAMETER_COUNT, current_weights, extract, generated_source, is_quiet, tuned_source_hash,
};

const DEFAULT_EPOCHS: usize = 100;
const DEFAULT_LEARNING_RATE: f64 = 4.0;
const DEFAULT_LOGISTIC_SCALE: f64 = 400.0;
const DEFAULT_L2: f64 = 1.0e-5;

struct Options {
    input: PathBuf,
    output: PathBuf,
    epochs: usize,
    learning_rate: f64,
    logistic_scale: f64,
    l2: f64,
}

struct Sample {
    board: Board,
    result: f64,
    validation: bool,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("texel: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let Some(options) = parse_options()? else {
        return Ok(());
    };
    let bytes = fs::read(&options.input)
        .map_err(|error| format!("cannot read {}: {error}", options.input.display()))?;
    let source_hash = fnv1a64(&bytes);
    let (samples, parsed, quiet) = load_samples(&bytes)?;
    if samples.is_empty() {
        return Err("the input contains no quiet, valid positions".into());
    }
    let training_count = samples.iter().filter(|sample| !sample.validation).count();
    let validation_count = samples.len() - training_count;
    if training_count == 0 {
        return Err("the deterministic split contains no training positions".into());
    }

    println!(
        "loaded {parsed} records; retained {quiet} quiet records and {} unique positions ({} train, {validation_count} validation)",
        samples.len(),
        training_count
    );
    println!(
        "input hash {source_hash:016x}; checked-in weight source {:016x}",
        tuned_source_hash()
    );

    let initial = current_weights();
    let mut weights = initial;
    for epoch in 0..options.epochs {
        let mut gradient = [0.0; PARAMETER_COUNT];
        let mut training_loss = 0.0;
        for sample in samples.iter().filter(|sample| !sample.validation) {
            let features = extract(&sample.board);
            let probability = sigmoid(features.value(&weights) / options.logistic_scale);
            training_loss += cross_entropy(probability, sample.result);
            features.add_gradient(
                &mut gradient,
                (probability - sample.result) / options.logistic_scale,
            );
        }
        let count = training_count as f64;
        for index in 0..PARAMETER_COUNT {
            let regularization = options.l2 * (weights[index] - initial[index]);
            weights[index] -= options.learning_rate * (gradient[index] / count + regularization);
        }
        if epoch == 0 || (epoch + 1) % 10 == 0 || epoch + 1 == options.epochs {
            let validation_loss = mean_loss(
                samples.iter().filter(|sample| sample.validation),
                &weights,
                options.logistic_scale,
            );
            if validation_count == 0 {
                println!(
                    "epoch {:>4}: train loss {:.6}",
                    epoch + 1,
                    training_loss / count
                );
            } else {
                println!(
                    "epoch {:>4}: train loss {:.6}, validation loss {validation_loss:.6}",
                    epoch + 1,
                    training_loss / count
                );
            }
        }
    }

    write_atomically(
        &options.output,
        generated_source(&weights, source_hash).as_bytes(),
    )?;
    println!("wrote {}", options.output.display());
    Ok(())
}

fn parse_options() -> Result<Option<Options>, String> {
    let mut args = env::args().skip(1);
    let Some(first) = args.next() else {
        print_usage();
        return Err("an input TSV path is required".into());
    };
    if first == "--help" || first == "-h" {
        print_usage();
        return Ok(None);
    }
    let mut options = Options {
        input: first.into(),
        output: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/eval_tuned.rs"),
        epochs: DEFAULT_EPOCHS,
        learning_rate: DEFAULT_LEARNING_RATE,
        logistic_scale: DEFAULT_LOGISTIC_SCALE,
        l2: DEFAULT_L2,
    };
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("{flag} requires a value"))?;
        match flag.as_str() {
            "--output" => options.output = value.into(),
            "--epochs" => options.epochs = parse_positive(&flag, &value)?,
            "--learning-rate" => options.learning_rate = parse_positive(&flag, &value)?,
            "--logistic-scale" => options.logistic_scale = parse_positive(&flag, &value)?,
            "--l2" => {
                options.l2 = value
                    .parse()
                    .map_err(|_| format!("invalid value for {flag}: {value}"))?;
                if options.l2 < 0.0 || !options.l2.is_finite() {
                    return Err(format!("{flag} must be finite and non-negative"));
                }
            }
            _ => return Err(format!("unknown option: {flag}")),
        }
    }
    Ok(Some(options))
}

fn parse_positive<T>(flag: &str, value: &str) -> Result<T, String>
where
    T: std::str::FromStr + PartialOrd + Default,
{
    let parsed = value
        .parse::<T>()
        .map_err(|_| format!("invalid value for {flag}: {value}"))?;
    if parsed <= T::default() {
        return Err(format!("{flag} must be positive"));
    }
    Ok(parsed)
}

fn print_usage() {
    eprintln!(
        "Usage: texel <positions.tsv> [--output <eval_tuned.rs>] [--epochs N] \
         [--learning-rate X] [--logistic-scale X] [--l2 X]\n\
         TSV rows are: <white result: 0, 0.5, or 1><TAB><FEN>"
    );
}

fn load_samples(bytes: &[u8]) -> Result<(Vec<Sample>, usize, usize), String> {
    let text =
        std::str::from_utf8(bytes).map_err(|error| format!("input is not UTF-8: {error}"))?;
    let mut unique: HashMap<u64, (Board, f64, usize)> = HashMap::new();
    let mut parsed = 0;
    let mut quiet = 0;
    for (line_index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (result, fen) = line
            .split_once('\t')
            .ok_or_else(|| format!("line {} has no tab separator", line_index + 1))?;
        let result: f64 = result
            .parse()
            .map_err(|_| format!("line {} has an invalid result", line_index + 1))?;
        if !matches!(result, 0.0 | 0.5 | 1.0) {
            return Err(format!(
                "line {} result must be 0, 0.5, or 1",
                line_index + 1
            ));
        }
        let board = fen
            .parse::<Board>()
            .map_err(|error| format!("line {} has an invalid FEN: {error}", line_index + 1))?;
        parsed += 1;
        if !is_quiet(&board) {
            continue;
        }
        quiet += 1;
        let entry = unique.entry(board.hash()).or_insert((board, 0.0, 0));
        entry.1 += result;
        entry.2 += 1;
    }
    let mut samples: Vec<_> = unique
        .into_iter()
        .map(|(hash, (board, result_sum, count))| Sample {
            board,
            result: result_sum / count as f64,
            validation: hash % 10 == 0,
        })
        .collect();
    samples.sort_unstable_by_key(|sample| sample.board.hash());
    Ok((samples, parsed, quiet))
}

fn mean_loss<'a>(
    samples: impl Iterator<Item = &'a Sample>,
    weights: &[f64; PARAMETER_COUNT],
    logistic_scale: f64,
) -> f64 {
    let mut loss = 0.0;
    let mut count = 0;
    for sample in samples {
        let probability = sigmoid(extract(&sample.board).value(weights) / logistic_scale);
        loss += cross_entropy(probability, sample.result);
        count += 1;
    }
    if count == 0 { 0.0 } else { loss / count as f64 }
}

fn sigmoid(value: f64) -> f64 {
    if value >= 0.0 {
        1.0 / (1.0 + (-value).exp())
    } else {
        let exp = value.exp();
        exp / (1.0 + exp)
    }
}

fn cross_entropy(probability: f64, result: f64) -> f64 {
    let probability = probability.clamp(1.0e-12, 1.0 - 1.0e-12);
    -result * probability.ln() - (1.0 - result) * (1.0 - probability).ln()
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn write_atomically(path: &Path, contents: &[u8]) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, contents)
        .map_err(|error| format!("cannot write {}: {error}", temporary.display()))?;
    fs::rename(&temporary, path).map_err(|error| {
        format!(
            "cannot replace {} with {}: {error}",
            path.display(),
            temporary.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loader_averages_duplicate_zobrist_positions() {
        let fen = Board::default().to_string();
        let input = format!("1\t{fen}\n0\t{fen}\n");
        let (samples, parsed, quiet) = load_samples(input.as_bytes()).unwrap();
        assert_eq!((parsed, quiet, samples.len()), (2, 2, 1));
        assert_eq!(samples[0].result, 0.5);
    }

    #[test]
    fn hash_is_stable() {
        assert_eq!(fnv1a64(b"hello"), 0xa430_d846_80aa_bd0b);
    }
}
