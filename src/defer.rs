pub struct Deferred<F: FnOnce()>(Option<F>);

impl<F: FnOnce()> Drop for Deferred<F> {
    fn drop(&mut self) {
        self.0.take().map(|f| f());
    }
}

impl<F: FnOnce()> Deferred<F> {
    pub fn disarm(mut self) {
        self.0.take();
    }
}

pub fn defer<F: FnOnce()>(f: F) -> Deferred<F> {
    Deferred(Some(f))
}
