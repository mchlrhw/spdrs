use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: spdrs <url>");
        return;
    }

    let url = args
        .get(1)
        .expect("the index must exist due to previous len check");

    println!("{url}");
}
