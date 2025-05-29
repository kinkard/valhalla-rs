use libvalhalla::GraphReader;

#[test]
fn empty() {
    assert!(GraphReader::new(Default::default()).is_none());
}
