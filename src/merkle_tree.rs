use super::Merge;
use std::collections::VecDeque;
use std::marker::PhantomData;

pub struct MerkleTree<T: Merge + Ord + Default + Clone> {
    nodes: Vec<T>,
}

impl<T: Merge + Ord + Default + Clone> MerkleTree<T> {
    pub fn build_proof(&self, indices: &[usize]) -> Option<MerkleProof<T>> {
        if self.nodes.is_empty() || indices.is_empty() {
            return None;
        }

        let leaves_count = (self.nodes.len() >> 1) + 1;
        let mut indices = indices
            .iter()
            .map(|i| leaves_count + i - 1)
            .collect::<Vec<_>>();

        indices.sort_by(|a, b| b.cmp(&a));

        if indices[0] >= (leaves_count << 1) - 1 {
            return None;
        }

        let mut lemmas = Vec::new();
        let mut queue = indices.iter().cloned().map(|i| i).collect::<VecDeque<_>>();

        while let Some(index) = queue.pop_front() {
            let sibling = calc_sibling(index);
            if Some(&sibling) == queue.front() {
                queue.pop_front();
            } else {
                lemmas.push(self.nodes[sibling].clone());
            }

            let parent = calc_parent(index);
            if parent != 0 {
                queue.push_back(parent);
            }
        }

        indices.sort_by(|a, b| self.nodes[*a].cmp(&self.nodes[*b]));

        let indices = indices.into_iter().map(|i| i as u32).collect::<Vec<_>>();
        Some(MerkleProof { indices, lemmas })
    }

    pub fn root(&self) -> T {
        if self.nodes.is_empty() {
            T::default()
        } else {
            self.nodes[0].clone()
        }
    }

    pub fn nodes(&self) -> &Vec<T> {
        &self.nodes
    }
}

pub struct MerkleProof<T: Merge + Ord + Default + Clone> {
    indices: Vec<u32>,
    lemmas: Vec<T>,
}

impl<T: Merge + Ord + Default + Clone> MerkleProof<T> {
    pub fn root(&self, leaves: &[T]) -> Option<T> {
        // TODO: Remove this clone
        let mut leaves = leaves.to_vec();
        if leaves.len() != self.indices.len() || leaves.is_empty() {
            return None;
        }
        leaves.sort();
        let mut pre = self
            .indices
            .iter()
            .zip(leaves.into_iter())
            .map(|(i, l)| (*i as usize, l))
            .collect::<Vec<_>>();
        pre.sort_by(|a, b| b.0.cmp(&a.0));

        let mut queue = pre.into_iter().collect::<VecDeque<_>>();
        let mut lemmas_iter = self.lemmas.iter();

        while let Some((index, node)) = queue.pop_front() {
            if index == 0 {
                // ensure that all lemmas and leaves are consumed
                if lemmas_iter.next().is_none() && queue.is_empty() {
                    return Some(node);
                } else {
                    return None;
                }
            }

            if let Some(sibling) = match queue.front() {
                Some((front, _)) if *front == calc_sibling(index) => queue.pop_front().map(|i| i.1),
                _ => lemmas_iter.next().cloned(),
            } {
                let parent_node = if is_left(index) {
                    T::merge(&node, &sibling)
                } else {
                    T::merge(&sibling, &node)
                };

                queue.push_back((calc_parent(index), parent_node));
            }
        }

        None
    }

    pub fn verify(&self, root: &T, leaves: &[T]) -> bool {
        match self.root(leaves) {
            Some(r) => &r == root,
            _ => false,
        }
    }

    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    pub fn lemmas(&self) -> &[T] {
        &self.lemmas
    }
}

#[derive(Default)]
pub struct CBMT<T: Merge + Ord + Default + Clone> {
    phantom: PhantomData<T>,
}

impl<T: Merge + Ord + Default + Clone> CBMT<T> {
    pub fn build_merkle_root(leaves: &[T]) -> T {
        if leaves.is_empty() {
            return T::default();
        }

        let mut queue = VecDeque::with_capacity((leaves.len() + 1) >> 1);

        let mut iter = leaves.rchunks_exact(2);
        while let Some([leaf1, leaf2]) = iter.next() {
            queue.push_back(T::merge(leaf1, leaf2))
        }
        if let [leaf] = iter.remainder() {
            queue.push_front(leaf.clone())
        }

        while queue.len() > 1 {
            let right = queue.pop_front().unwrap();
            let left = queue.pop_front().unwrap();
            queue.push_back(T::merge(&left, &right));
        }

        queue.pop_front().unwrap()
    }

    pub fn build_merkle_tree(leaves: Vec<T>) -> MerkleTree<T> {
        let len = leaves.len();
        if len > 0 {
            let mut nodes = vec![T::default(); len - 1];
            nodes.extend(leaves);

            (0..len - 1)
                .rev()
                .for_each(|i| nodes[i] = T::merge(&nodes[(i << 1) + 1], &nodes[(i << 1) + 2]));

            MerkleTree { nodes }
        } else {
            MerkleTree { nodes: vec![] }
        }
    }

    pub fn build_merkle_proof(leaves: &[T], indices: &[usize]) -> Option<MerkleProof<T>> {
        // TODO: Remove this clone
        Self::build_merkle_tree(leaves.to_vec()).build_proof(indices)
    }
}

pub fn calc_sibling(num: usize) -> usize {
    if num == 0 {
        0
    } else {
        ((num + 1) ^ 1) - 1
    }
}

pub fn calc_parent(num: usize) -> usize {
    if num == 0 {
        0
    } else {
        (num - 1) >> 1
    }
}

pub fn is_left(num: usize) -> bool {
    num & 1 == 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::collection::vec;
    use proptest::num::i32;
    use proptest::prelude::*;
    use proptest::sample::subsequence;
    use proptest::{proptest, proptest_helper};

    impl Merge for i32 {
        fn merge(left: &Self, right: &Self) -> Self {
            right.wrapping_sub(*left)
        }
    }

    #[test]
    fn build_empty() {
        let leaves = vec![];
        let tree = CBMT::<i32>::build_merkle_tree(leaves);
        assert!(tree.nodes().is_empty());
        assert_eq!(tree.root(), i32::default());
    }

    #[test]
    fn build_one() {
        let leaves = vec![1i32];
        let tree = CBMT::build_merkle_tree(leaves);
        assert_eq!(&vec![1], tree.nodes());
    }

    #[test]
    fn build_two() {
        let leaves = vec![1i32, 2];
        let tree = CBMT::build_merkle_tree(leaves);
        assert_eq!(&vec![1, 1, 2], tree.nodes());
    }

    #[test]
    fn build_five() {
        let leaves = vec![2i32, 3, 5, 7, 11];
        let tree = CBMT::build_merkle_tree(leaves);
        assert_eq!(&vec![4, -2, 2, 4, 2, 3, 5, 7, 11], tree.nodes());
    }

    #[test]
    fn build_root_directly() {
        let leaves = vec![2i32, 3, 5, 7, 11];
        assert_eq!(4, CBMT::build_merkle_root(&leaves));
    }

    fn _build_root_is_same_as_tree_root(leaves: Vec<i32>) {
        let root = CBMT::build_merkle_root(&leaves);
        let tree = CBMT::build_merkle_tree(leaves);
        assert_eq!(root, tree.root());
    }

    proptest! {
        #[test]
        fn build_root_is_same_as_tree_root(leaves in vec(i32::ANY,  0..1000)) {
            _build_root_is_same_as_tree_root(leaves);
        }
    }

    #[test]
    fn build_proof() {
        let leaves = vec![2i32, 3, 5, 7, 11, 13];
        let indices = vec![0, 5];
        let proof_leaves = indices
            .iter()
            .map(|i| leaves[*i].clone())
            .collect::<Vec<_>>();
        let proof = CBMT::build_merkle_proof(&leaves, &indices).unwrap();
        assert_eq!(vec![11, 3, 2], proof.lemmas);
        assert_eq!(Some(1), proof.root(&proof_leaves));
    }

    fn _tree_root_is_same_as_proof_root(leaves: Vec<i32>, indices: Vec<usize>) {
        let proof_leaves = indices
            .iter()
            .map(|i| leaves[*i].clone())
            .collect::<Vec<_>>();
        let proof = CBMT::build_merkle_proof(&leaves, &indices).unwrap();
        let root = CBMT::build_merkle_root(&leaves);
        assert_eq!(root, proof.root(&proof_leaves).unwrap());
    }

    proptest! {
        #[test]
        fn tree_root_is_same_as_proof_root(input in vec(i32::ANY,  2..1000)
            .prop_flat_map(|leaves| (Just(leaves.clone()), subsequence((0..leaves.len()).collect::<Vec<usize>>(), 1..leaves.len())))
        ) {
            _tree_root_is_same_as_proof_root(input.0, input.1);
        }
    }
}
