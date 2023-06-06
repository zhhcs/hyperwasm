use std::marker::PhantomData;

pub struct Task<S: 'static> {
    _p: PhantomData<S>,
}

impl<S: 'static> Task<S> {}
