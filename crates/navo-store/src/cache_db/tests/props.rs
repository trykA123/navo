use std::collections::HashMap;

use proptest::prelude::*;

use super::*;

const TABLES: [(&str, u64); 2] = [("thumbs", 48), ("sizes", 64)];

#[derive(Debug, Clone)]
enum Op {
    Put { table: usize, key: u8, len: usize },
    Get { table: usize, key: u8 },
    Remove { table: usize, key: u8 },
}

fn op() -> impl Strategy<Value = Op> {
    let table = 0..TABLES.len();
    let key = 0_u8..6;
    prop_oneof![
        3 => (table.clone(), key.clone(), 0_usize..72)
            .prop_map(|(table, key, len)| Op::Put { table, key, len }),
        2 => (table.clone(), key.clone()).prop_map(|(table, key)| Op::Get { table, key }),
        1 => (table, key).prop_map(|(table, key)| Op::Remove { table, key }),
    ]
}

fn key_bytes(key: u8) -> Vec<u8> {
    vec![key; usize::from(key % 3) + 1]
}

#[derive(Default)]
struct Model {
    entries: HashMap<(usize, Vec<u8>), (Vec<u8>, u64)>,
    clock: u64,
}

impl Model {
    fn used(&self, table: usize) -> u64 {
        self.entries
            .iter()
            .filter(|((t, _), _)| *t == table)
            .map(|((_, key), (value, _))| (key.len() + value.len()) as u64)
            .sum()
    }

    fn put(&mut self, table: usize, key: Vec<u8>, value: Vec<u8>) {
        self.clock += 1;
        if (key.len() + value.len()) as u64 > TABLES[table].1 {
            self.entries.remove(&(table, key));
            return;
        }
        self.entries.insert((table, key), (value, self.clock));
        while self.used(table) > TABLES[table].1 {
            let oldest = self
                .entries
                .iter()
                .filter(|((t, _), _)| *t == table)
                .min_by_key(|(_, (_, accessed))| *accessed)
                .map(|(id, _)| id.clone())
                .unwrap();
            self.entries.remove(&oldest);
        }
    }

    fn get(&mut self, table: usize, key: Vec<u8>) -> Option<Vec<u8>> {
        self.clock += 1;
        let (value, accessed) = self.entries.get_mut(&(table, key))?;
        *accessed = self.clock;
        Some(value.clone())
    }

    fn remove(&mut self, table: usize, key: Vec<u8>) -> bool {
        self.entries.remove(&(table, key)).is_some()
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn cache_matches_an_lru_model(ops in prop::collection::vec(op(), 1..60)) {
        let mut cache = mem();
        for (name, budget) in TABLES {
            cache.set_budget(name, budget);
        }
        let mut model = Model::default();
        for (step, op) in ops.into_iter().enumerate() {
            match op {
                Op::Put { table, key, len } => {
                    let value = vec![step as u8; len];
                    cache.put(TABLES[table].0, &key_bytes(key), &value)?;
                    model.put(table, key_bytes(key), value);
                }
                Op::Get { table, key } => {
                    let got = cache.get(TABLES[table].0, &key_bytes(key))?;
                    prop_assert_eq!(got, model.get(table, key_bytes(key)), "step {}", step);
                }
                Op::Remove { table, key } => {
                    let removed = cache.remove(TABLES[table].0, &key_bytes(key))?;
                    prop_assert_eq!(removed, model.remove(table, key_bytes(key)), "step {}", step);
                }
            }
            for (table, (name, budget)) in TABLES.into_iter().enumerate() {
                let used = cache.used_bytes(name)?;
                prop_assert!(used <= budget, "step {}: {} uses {} > {}", step, name, used, budget);
                prop_assert_eq!(used, model.used(table), "step {}: {}", step, name);
            }
        }
    }
}
