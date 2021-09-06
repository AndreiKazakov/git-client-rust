use std::fmt::{Debug, Formatter};

pub struct GitError(pub String);

impl Debug for GitError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<A: ToString> From<A> for GitError {
    fn from(a: A) -> Self {
        GitError(a.to_string())
    }
}
