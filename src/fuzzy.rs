//! Basic string fuzzy-matching implementation based on the Levenshtein distance algorithm.
use std::collections::HashSet;

#[derive(Debug, Clone, Default)]
pub struct Fuzzy {
    set: HashSet<String>,
}

impl Fuzzy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, s: impl AsRef<str>) {
        self.set.insert(
            s.as_ref().to_owned()
        );
    }

    pub fn find(&self, input: impl AsRef<str>) -> Vec<(usize, &str)> {
        let input   = input.as_ref();
        let mut out = vec![];

        for s in &self.set {
            let pair = (
                Self::levenshtein(s, input),
                s.as_str()
            );

            out.push(pair)
        }

        out.sort_unstable_by_key(|s| s.0);

        out
    }

    // Adapted from https://stackoverflow.com/a/9453762
    fn levenshtein(a: &str, b: &str) -> usize {
        use std::cmp::{min, max};

        if a.is_empty() || b.is_empty() {
            return max(
                a.len(), b.len()
            );
        }

        #[allow(clippy::zero_repeat_side_effects)]
        let mut distances = vec![
            vec![0; b.len() + 1]; a.len() + 1
        ];

        #[allow(clippy::needless_range_loop)]
        for i in 0..=a.len() { distances[i][0] = i; }
        #[allow(clippy::needless_range_loop)]
        for i in 0..=b.len() { distances[0][i] = i; }
        
        let b_a = a.as_bytes();
        let b_b = b.as_bytes();

        for i in 1..=a.len() {
            for j in 1..=b.len() {
                let cost = if b_b[j - 1] == b_a[i - 1] { 0 } else { 1 };

                distances[i][j] = min(
                    min(distances[i - 1][j] + 1, distances[i][j - 1] + 1),
                    distances[i - 1][j - 1] + cost
                );
            }
        }

        distances[a.len()][b.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic() {
        let mut f = Fuzzy::new();

        f.add("rust");
        f.add("rst");
        f.add("cst");
        f.add("ooo");
        f.add("bat");

        let r = f.find("rust");

        assert_eq!(
            r[0], (0, "rust")
        );
        
        assert_eq!(
            r[1], (1, "rst")
        );

        assert_eq!(
            r[4], (4, "ooo")
        );
    }
}