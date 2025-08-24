use super::{Storage, Swappable};

#[derive(Clone, PartialEq, Debug)]
struct DummyRow(Vec<u8>);

impl Swappable for DummyRow {}

#[test]
fn test_storage_push_and_get() {
    let mut storage: Storage<DummyRow> = Storage::new(2);
    let row1 = DummyRow(vec![1, 2, 3]);
    let row2 = DummyRow(vec![4, 5, 6]);

    storage.push(row1.clone());
    storage.push(row2.clone());

    assert_eq!(storage.len(), 2);
    assert_eq!(storage.get(0).unwrap(), &row1);
    assert_eq!(storage.get(1).unwrap(), &row2);
}

#[test]
fn test_storage_swap() {
    let mut storage: Storage<DummyRow> = Storage::new(2);
    let row1 = DummyRow(vec![1]);
    let row2 = DummyRow(vec![2]);
    storage.push(row1.clone());
    storage.push(row2.clone());

    storage.swap(0, 1);

    assert_eq!(storage.get(0).unwrap(), &row2);
    assert_eq!(storage.get(1).unwrap(), &row1);
}

#[test]
fn test_storage_swap_out_of_bounds() {
    let mut storage: Storage<DummyRow> = Storage::new(1);
    storage.push(DummyRow(vec![1]));
    // Should not panic
    storage.swap(0, 1);
    storage.swap(1, 0);
}

