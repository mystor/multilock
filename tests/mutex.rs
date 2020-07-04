use multilock::multilock;
use parking_lot::Mutex;

#[test]
fn test_mutex() {
    let m1 = Mutex::new(5);
    let m2 = Mutex::new("cheese");

    multilock(|mut builder| {
        let mut m1_token = builder.add(&m1);
        let mut m2_token = builder.add(&m2);

        assert!(!m1.is_locked());
        assert!(!m2.is_locked());

        let locker = builder.finish();

        assert!(m1.is_locked());
        assert!(m2.is_locked());
        assert_eq!(*m1_token.get(&locker), 5);
        assert_eq!(*m2_token.get(&locker), "cheese");

        *m1_token.get_mut(&locker) = 10;
        *m2_token.get_mut(&locker) = "pies";

        drop(locker);

        assert!(!m1.is_locked());
        assert!(!m2.is_locked());
    });

    assert!(!m1.is_locked());
    assert!(!m2.is_locked());

    assert_eq!(m1.into_inner(), 10);
    assert_eq!(m2.into_inner(), "pies");
}
