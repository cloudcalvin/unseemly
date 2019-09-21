use runtime::reify::Reifiable;
use std::{clone::Clone, fmt, rc::Rc};

// Potential optimization: replace a run of ten nodes with a `HashMap`.
// Recursively replace runs of those, too...

custom_derive! {
    /// A functional key-value map. Seaching is linear (boo!), but the map is persistant (yay!).
    /// (It's just a linked list of pairs.)
    #[must_use] // this is a functional data structure; dropping it on the floor is usually bad
    #[derive(Reifiable, Default)]
    pub struct Assoc<K, V> {
        n: Option<Rc<AssocNode<K, V>>> // This could be a newtype, except for `custom_derive`
    }
}

impl<K: Clone, V: Clone> Clone for Assoc<K, V> {
    fn clone(&self) -> Assoc<K, V> { Assoc { n: self.n.clone() } }
}

impl<K: PartialEq + Clone, V: PartialEq> PartialEq for Assoc<K, V> {
    fn eq(&self, other: &Assoc<K, V>) -> bool {
        for (k, v) in self.iter_pairs() {
            if let Some(other_v) = other.find(k) {
                if !(v == other_v) {
                    return false;
                }
            } else {
                return false;
            }
        }

        for (other_k, other_v) in other.iter_pairs() {
            if let Some(v) = self.find(other_k) {
                if !(v == other_v) {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }
}

impl<K: PartialEq + Clone, V: PartialEq> Eq for Assoc<K, V> {}

custom_derive! {
    #[derive(Reifiable, Clone)]
    pub struct AssocNode<K, V> {
        k: K,
        v: V,
        next: Assoc<K,V>
    }
}

impl<K: PartialEq, V> Assoc<K, V> {
    /// Possibly unintuitively, all empty assocs are identical.
    pub fn almost_ptr_eq(&self, other: &Assoc<K, V>) -> bool {
        match (&self.n, &other.n) {
            (&None, &None) => true,
            (&Some(ref l_rc), &Some(ref r_rc)) => {
                &**l_rc as *const AssocNode<K, V> == &**r_rc as *const AssocNode<K, V>
            }
            _ => false,
        }
    }

    pub fn find<'assoc, 'f>(&'assoc self, target: &'f K) -> Option<&'assoc V> {
        match self.n {
            None => None,
            Some(ref node) => {
                if (*node).k == *target {
                    Some(&node.v)
                } else {
                    (*node).next.find(target)
                }
            }
        }
    }

    pub fn empty(&self) -> bool { self.n.is_none() }

    pub fn set(&self, k: K, v: V) -> Assoc<K, V> {
        Assoc { n: Some(Rc::new(AssocNode { k: k, v: v, next: Assoc { n: self.n.clone() } })) }
    }

    pub fn new() -> Assoc<K, V> { Assoc { n: None } }

    pub fn single(k: K, v: V) -> Assoc<K, V> { Self::new().set(k, v) }

    pub fn iter_pairs(&self) -> PairIter<K, V> { PairIter { seen: Assoc::new(), cur: self } }

    pub fn reduce<Out>(&self, red: &dyn Fn(&K, &V, Out) -> Out, base: Out) -> Out {
        match self.n {
            None => base,
            Some(ref node) => red(&node.k, &node.v, node.next.reduce(red, base)),
        }
    }
}

impl<K: PartialEq + Clone, V> Assoc<K, V> {
    pub fn iter_keys<'assoc>(&'assoc self) -> Box<dyn Iterator<Item = K> + 'assoc> {
        Box::new(self.iter_pairs().map(|p| (*p.0).clone()))
    }
}

impl<K: PartialEq + Clone, V: Clone> Assoc<K, V> {
    pub fn iter_values<'assoc>(&'assoc self) -> Box<dyn Iterator<Item = V> + 'assoc> {
        Box::new(self.iter_pairs().map(|p| (*p.1).clone()))
    }

    pub fn map<NewV, F>(&self, mut f: F) -> Assoc<K, NewV>
    where F: FnMut(&V) -> NewV {
        self.map_borrow_f(&mut f)
    }

    pub fn map_borrow_f<NewV, F>(&self, f: &mut F) -> Assoc<K, NewV>
    where F: FnMut(&V) -> NewV {
        match self.n {
            None => Assoc { n: None },
            Some(ref node) => Assoc {
                n: Some(Rc::new(AssocNode {
                    k: node.k.clone(),
                    v: f(&node.v),
                    next: node.next.map_borrow_f(f),
                })),
            },
        }
    }

    pub fn keyed_map_borrow_f<NewV, F>(&self, f: &mut F) -> Assoc<K, NewV>
    where F: FnMut(&K, &V) -> NewV {
        match self.n {
            None => Assoc { n: None },
            Some(ref node) => Assoc {
                n: Some(Rc::new(AssocNode {
                    k: node.k.clone(),
                    v: f(&node.k, &node.v),
                    next: node.next.keyed_map_borrow_f(f),
                })),
            },
        }
    }

    // TODO: this should handle missing keys symmetrically
    pub fn map_with<NewV>(
        &self,
        other: &Assoc<K, V>,
        f: &dyn Fn(&V, &V) -> NewV,
    ) -> Assoc<K, NewV>
    {
        match self.n {
            None => Assoc { n: None },
            Some(ref node) => {
                Assoc {
                    n: Some(Rc::new(AssocNode {
                        k: node.k.clone(),
                        // Should we require `K` and `V` to be `Debug` to use `find_or_panic`?
                        v: f(&node.v, other.find(&node.k).unwrap()),
                        next: node.next.map_with(other, f),
                    })),
                }
            }
        }
    }

    pub fn keyed_map_with<NewV>(
        &self,
        other: &Assoc<K, V>,
        f: &dyn Fn(&K, &V, &V) -> NewV,
    ) -> Assoc<K, NewV>
    {
        match self.n {
            None => Assoc { n: None },
            Some(ref node) => {
                Assoc {
                    n: Some(Rc::new(AssocNode {
                        k: node.k.clone(),
                        // Should we require `K` and `V` to be `Debug` to use `find_or_panic`?
                        v: f(&node.k, &node.v, other.find(&node.k).unwrap()),
                        next: node.next.keyed_map_with(other, f),
                    })),
                }
            }
        }
    }
}

impl<K, V: PartialEq> Assoc<K, V> {
    pub fn find_value<'assoc, 'f>(&'assoc self, target: &'f V) -> Option<&'assoc K> {
        match self.n {
            None => None,
            Some(ref node) => {
                if (*node).v == *target {
                    Some(&node.k)
                } else {
                    (*node).next.find_value(target)
                }
            }
        }
    }
}

impl<K: PartialEq + fmt::Debug + Clone, V: fmt::Debug + Clone> Assoc<K, V> {
    pub fn find_or_panic<'assoc, 'f>(&'assoc self, target: &'f K) -> &'assoc V {
        match self.find(target) {
            None => panic!("{:#?} not found in {:#?}", target, self.map(|_| "…")),
            Some(v) => v,
        }
    }
}

impl<K: PartialEq + Clone + fmt::Debug, V: fmt::Debug> fmt::Debug for Assoc<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "⟦")?;
        let mut first = true;
        for (k, v) in self.iter_pairs() {
            if !first {
                write!(f, ", ")?;
            }
            write!(f, "{:#?} ⇒ {:#?}", k, v)?;
            first = false;
        }
        write!(f, "⟧")
    }
}

impl<K: PartialEq + Clone + fmt::Display, V: fmt::Display> fmt::Display for Assoc<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "⟦")?;
        let mut first = true;
        for (k, v) in self.iter_pairs() {
            if !first {
                write!(f, ", ")?;
            }
            write!(f, "{} ⇒ {}", k, v)?;
            first = false;
        }
        write!(f, "⟧")
    }
}

impl<K: PartialEq + Clone, V: Clone> Assoc<K, V> {
    pub fn set_assoc(&self, other: &Assoc<K, V>) -> Assoc<K, V> {
        match other.n {
            None => (*self).clone(),
            Some(ref node) => self.set_assoc(&node.next).set(node.k.clone(), node.v.clone()),
        }
    }

    /// Generates a version of `self` that lacks the common suffix it shares with `other` (if any)
    pub fn cut_common(&self, other: &Assoc<K, V>) -> Assoc<K, V> {
        match self.n {
            None => Assoc::new(),
            Some(ref node) => {
                if let Some(other_v) = other.find(&node.k) {
                    if &node.v as *const V == other_v as *const V {
                        return Assoc::new(); // we found the common suffix
                    }
                }
                node.next.cut_common(other).set(node.k.clone(), node.v.clone())
            }
        }
    }

    pub fn unset(&self, k: &K) -> Assoc<K, V> {
        match self.n {
            None => Assoc { n: None },
            Some(ref node) => {
                let v = node.v.clone();
                if &node.k != k {
                    Assoc {
                        n: Some(Rc::new(AssocNode {
                            k: node.k.clone(),
                            v: v,
                            next: node.next.unset(k),
                        })),
                    }
                } else {
                    node.next.unset(k)
                }
            }
        }
    }
    // This isn't right without deduplication before hand...
    // pub fn filter(&self, f: &Fn(&V) -> bool) -> Assoc<K, V> {
    //     match self.n {
    //         None => Assoc{ n: None },
    //         Some(ref node) => {
    //             let v = node.v.clone();
    //             if f(&v) {
    //                 Assoc{
    //                     n: Some(Rc::new(AssocNode {
    //                         k: node.k.clone(), v: v,
    //                         next: node.next.filter(f)
    //                     }))
    //                 }
    //             } else {
    //                 node.next.filter(f)
    //             }
    //         }
    //     }
    // }
}

pub struct KeyIter<'assoc, K: PartialEq + 'assoc, V: 'assoc> {
    cur: &'assoc Assoc<K, V>,
}

impl<'assoc, K: PartialEq, V> Iterator for KeyIter<'assoc, K, V> {
    type Item = &'assoc K;
    fn next(&mut self) -> Option<&'assoc K> {
        match self.cur.n {
            None => None,
            Some(ref node) => {
                self.cur = &(*node).next;
                Some(&(*node).k)
            }
        }
    }
}

pub struct ValueIter<'assoc, K: PartialEq + 'assoc, V: 'assoc> {
    cur: &'assoc Assoc<K, V>,
}

impl<'assoc, K: PartialEq, V> Iterator for ValueIter<'assoc, K, V> {
    type Item = &'assoc V;
    fn next(&mut self) -> Option<&'assoc V> {
        match self.cur.n {
            None => None,
            Some(ref node) => {
                self.cur = &(*node).next;
                Some(&(*node).v)
            }
        }
    }
}

pub struct PairIter<'assoc, K: PartialEq + 'assoc, V: 'assoc> {
    seen: Assoc<K, ()>, // TODO: this should probably be a HashMap to avoid quadratic behavior.
    cur: &'assoc Assoc<K, V>,
}

impl<'assoc, K: PartialEq + Clone, V> Iterator for PairIter<'assoc, K, V> {
    type Item = (&'assoc K, &'assoc V);
    fn next(&mut self) -> Option<(&'assoc K, &'assoc V)> {
        match self.cur.n {
            None => None,
            Some(ref node) => {
                self.cur = &(*node).next;
                if self.seen.find(&(*node).k).is_none() {
                    // have we done this key already?
                    self.seen = self.seen.set((*node).k.clone(), ());
                    Some((&(*node).k, &(*node).v))
                } else {
                    self.next() // try the next one
                }
            }
        }
    }
}

#[test]
fn basic_assoc() {
    let mt: Assoc<i32, i32> = Assoc::new();
    let a1 = mt.set(5, 6);
    let a2 = a1.set(6, 7);
    let a_override = a2.set(5, 500);

    assert_eq!(mt.find(&5), None);
    assert_eq!(a1.find(&6), None);
    assert_eq!(a2.find(&999), None);
    assert_eq!(a_override.find(&999), None);
    assert_eq!(a1.find(&5), Some(&6));
    assert_eq!(a2.find(&5), Some(&6));
    assert_eq!(a2.find(&6), Some(&7));
    assert_eq!(a2.find(&5), Some(&6));
    assert_eq!(a_override.find(&5), Some(&500));
    assert_eq!(a_override.find(&6), Some(&7));

    assert_eq!(a_override.unset(&5).find(&5), None);
    assert_eq!(a_override.unset(&6).find(&6), None);

    assert_eq!(a_override.unset(&6).find(&5), Some(&500));
    assert_eq!(a_override.unset(&5).find(&6), Some(&7));

    assert_eq!(a_override.unset(&-111).find(&5), Some(&500));
}

#[test]
fn assoc_equality() {
    let mt: Assoc<i32, i32> = Assoc::new();
    let a1 = mt.set(5, 6);
    let a2 = a1.set(6, 7);
    let a_override = a2.set(5, 500);

    let a2_opposite = mt.set(6, 7).set(5, 6);
    let a_override_direct = mt.set(5, 500).set(6, 7);

    assert_eq!(mt, Assoc::new());
    assert_eq!(a1, a1);
    assert!(a1 != mt);
    assert!(mt != a1);
    assert_eq!(a2, a2);
    assert_eq!(a2, a2_opposite);
    assert_eq!(a_override, a_override_direct);
    assert!(a2 != a_override);

    let a1_again = mt.set(5, 6);

    // Nothing shared: no-op
    assert_eq!(mt.cut_common(&mt), mt);
    assert_eq!(a1.cut_common(&mt), a1);
    assert_eq!(mt.cut_common(&a1), mt);
    assert_eq!(a1_again.cut_common(&a1), a1_again); // Needs pointer equality!
    assert_eq!(a_override_direct.cut_common(&a_override), a_override_direct);
    assert_eq!(a_override.cut_common(&a_override_direct), a_override);

    // Everything shared: empty result
    assert_eq!(a1.cut_common(&a1), mt);
    assert_eq!(a2.cut_common(&a2), mt);

    // Partial share:
    assert_eq!(a2.cut_common(&a1), mt.set(6, 7));
    assert_eq!(a_override.cut_common(&a2), mt.set(5, 500));
}

#[test]
fn assoc_r_and_r_roundtrip() {
    use num::BigInt;
    let mt: Assoc<BigInt, BigInt> = Assoc::new();
    let a1 = mt.set(BigInt::from(5), BigInt::from(6));
    let a2 = a1.set(BigInt::from(6), BigInt::from(7));

    assert_eq!(mt, Assoc::<BigInt, BigInt>::reflect(&mt.reify()));
    assert_eq!(a2, Assoc::<BigInt, BigInt>::reflect(&a2.reify()));
}

#[test]
fn assoc_map() {
    let a1 = assoc_n!("x" => 1, "y" => 2, "z" => 3);
    assert_eq!(a1.map(|a| a + 1), assoc_n!("x" => 2, "y" => 3, "z" => 4));

    let a2 = assoc_n!("y" => -2, "z" => -3, "x" => -1);
    assert_eq!(a1.map_with(&a2, &|a, b| a + b), assoc_n!("x" => 0, "y" => 0, "z" => 0));
}

#[test]
fn assoc_reduce() {
    let a1 = assoc_n!("x" => 1, "y" => 2, "z" => 3);
    assert_eq!(a1.reduce(&|_key, a, b| a + b, 0), 6);

    let a1 = assoc_n!("x" => 1, "y" => 2, "z" => 3);
    assert_eq!(a1.reduce(&|key, a, b| if key.is("y") { b } else { a + b }, 0), 4);
}
