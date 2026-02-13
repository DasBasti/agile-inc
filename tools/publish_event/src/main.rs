use std::env;
fn main(){
    let args:Vec<String> = env::args().collect();
    println!("publish_event stub: args={:?}", args);
}
