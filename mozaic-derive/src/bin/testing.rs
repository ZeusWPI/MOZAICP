
extern crate proc_macro;
extern crate syn;
#[macro_use]
extern crate quote;

#[macro_use]
extern crate mozaic_derive;


minimal!{comb3, 3}

fn main() {
    println!("hallo world {}", comb3());
}
