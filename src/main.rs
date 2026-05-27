use jk::cli::parse_argv;
use jk::output::{init_local_offset, Out};
use jk::run::run;

fn main() {
    // Capture the local UTC offset before any threads are spawned; see LOCAL_OFFSET in output.rs.
    init_local_offset();

    let argv: Vec<String> = std::env::args().skip(1).collect();
    let out = Out::from_env();

    let cli = match parse_argv(argv) {
        Ok(c) => c,
        Err(e) => {
            out.user_error(&e.to_string());
            std::process::exit(1);
        }
    };

    match run(cli, &out) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            out.user_error(&e.to_string());
            std::process::exit(1);
        }
    }
}
