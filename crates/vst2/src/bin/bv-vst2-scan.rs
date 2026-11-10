//! Looks at VST 2 libraries in a process of its own (the tests' child;
//! BananaV itself does it with `--vst2-scan`).

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args = match args.first().map(String::as_str) {
        Some(bv_vst2::scan::CHILD_FLAG) => &args[1..],
        _ => &args[..],
    };
    std::process::exit(bv_vst2::scan::child_main(args));
}
