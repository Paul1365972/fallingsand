fn main() {
    let a: Vec<i32> = (0..1000).collect();
    let mut b: Vec<i32> = Vec::new();

    a.into_iter().filter(|x| x % 3 == 0).for_each(|x| b.push(x));

    println!("{}", b.len());
}
