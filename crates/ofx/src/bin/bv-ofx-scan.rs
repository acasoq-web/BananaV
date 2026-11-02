//! Describes and tries OFX plug-ins in a process of its own (the tests'
//! child; BananaV itself does it with `--ofx-scan`).

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args = match args.first().map(String::as_str) {
        Some(bv_ofx::scan::CHILD_FLAG) => &args[1..],
        _ => &args[..],
    };
    std::process::exit(bv_ofx::scan::child_main(args));
}
