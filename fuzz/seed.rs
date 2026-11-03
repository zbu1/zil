use std::fs;

fn main() {
    let mut seeds: Vec<String> = fs::read_dir("../examples").unwrap().map(|e| fs::read_to_string(e.unwrap().path()).unwrap()).collect();
    seeds.extend(zil::all_examples().into_iter().map(String::from));
    for target in ["parse", "eval"] {
        let dir = format!("corpus/{target}");
        fs::create_dir_all(&dir).unwrap();
        for (i, s) in seeds.iter().enumerate() {
            fs::write(format!("{dir}/seed{i}"), s).unwrap();
        }
    }
    println!("{} seeds", seeds.len());
}
