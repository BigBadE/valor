use std::hash::Hash;

use crate::{Database, DependencyContext};

/// Trait for defining queries.
pub trait Query: 'static {
    type Key: Clone + Hash + Eq + 'static;
    type Value: Clone + Send + Sync + 'static;

    fn execute(db: &Database, key: Self::Key, ctx: &mut DependencyContext) -> Self::Value;
}

// Implement Query for tuple of 2 queries
impl<Q1, Q2> Query for (Q1, Q2)
where
    Q1: Query,
    Q2: Query<Key = Q1::Key>,
{
    type Key = Q1::Key;
    type Value = (Q1::Value, Q2::Value);

    fn execute(db: &Database, key: Self::Key, ctx: &mut DependencyContext) -> Self::Value {
        (db.query::<Q1>(key.clone(), ctx), db.query::<Q2>(key, ctx))
    }
}

// Implement Query for tuple of 3 queries
impl<Q1, Q2, Q3> Query for (Q1, Q2, Q3)
where
    Q1: Query,
    Q2: Query<Key = Q1::Key>,
    Q3: Query<Key = Q1::Key>,
{
    type Key = Q1::Key;
    type Value = (Q1::Value, Q2::Value, Q3::Value);

    fn execute(db: &Database, key: Self::Key, ctx: &mut DependencyContext) -> Self::Value {
        (
            db.query::<Q1>(key.clone(), ctx),
            db.query::<Q2>(key.clone(), ctx),
            db.query::<Q3>(key, ctx),
        )
    }
}

// Implement Query for tuple of 4 queries
impl<Q1, Q2, Q3, Q4> Query for (Q1, Q2, Q3, Q4)
where
    Q1: Query,
    Q2: Query<Key = Q1::Key>,
    Q3: Query<Key = Q1::Key>,
    Q4: Query<Key = Q1::Key>,
{
    type Key = Q1::Key;
    type Value = (Q1::Value, Q2::Value, Q3::Value, Q4::Value);

    fn execute(db: &Database, key: Self::Key, ctx: &mut DependencyContext) -> Self::Value {
        (
            db.query::<Q1>(key.clone(), ctx),
            db.query::<Q2>(key.clone(), ctx),
            db.query::<Q3>(key.clone(), ctx),
            db.query::<Q4>(key, ctx),
        )
    }
}
