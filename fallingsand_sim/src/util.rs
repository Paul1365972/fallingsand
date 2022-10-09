pub trait DrainFilterMap<T> {
    fn drain_filter_map<E, R, P, F, U>(&mut self, extractor: E, filter: P, mapper: F) -> Vec<U>
    where
        E: FnMut(&mut T) -> R,
        P: FnMut(&mut T, R) -> bool,
        F: FnMut(T, R) -> U;
}

impl<T> DrainFilterMap<T> for Vec<T> {
    fn drain_filter_map<E, R, P, F, U>(&mut self, extractor: E, filter: P, mapper: F) -> Vec<U>
    where
        E: FnMut(&mut T) -> R,
        P: FnMut(&mut T, R) -> bool,
        F: FnMut(T, R) -> U,
    {
        let result = Vec::new();
        let index = 0;
        let len = self.len();
        while index < len {
            let ele = self.get_mut(index).unwrap();
            let extract = extractor(ele);
            if filter(ele, extract) {
                result.push(mapper(self.remove(index), extract));
                len -= 1;
            } else {
                index += 1;
            }
        }
        result
    }
}
