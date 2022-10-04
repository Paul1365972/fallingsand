mod chunk;
mod coords;
mod tile;
mod simulator;

pub fn add(left: usize, right: usize) -> usize {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(3, 2);
        assert_eq!(result, 4);
    }
}
