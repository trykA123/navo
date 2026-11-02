use proptest::prelude::*;

use super::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]

    #[test]
    fn open_migrates_any_older_version(start in 0..=Db::VERSION) {
        let dir = tmp();
        let path = db_path(&dir);
        build_at_version(&path, start);
        let (db, report) = Db::open(&path, false)?;
        prop_assert_eq!((report.from_version, db.version()?), (start, Db::VERSION));
    }
}
