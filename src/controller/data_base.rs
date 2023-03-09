pub mod model;
use sea_orm::{Database, DatabaseConnection, DbErr};
use std::{
    mem::MaybeUninit,
    sync::{Arc, Once},
};

static ONCE_STATE: Once = Once::new();
static mut DB_POOL: MaybeUninit<Arc<DatabaseConnection>> = MaybeUninit::uninit();

pub(crate) trait DbErrToDbErrWrapper<T> {
    fn aynhow_result(self) -> anyhow::Result<T>;
}
impl<T> DbErrToDbErrWrapper<T> for Result<T, DbErr> {
    fn aynhow_result(self) -> anyhow::Result<T> {
        match self {
            Ok(r) => Ok(r),
            Err(e) => Err(anyhow::format_err!(e.to_string()))
        }
    }
}

pub async fn make_db_pool(protocol: &str) -> anyhow::Result<()> {
    let db: Arc<DatabaseConnection> =
        Arc::new(Database::connect(protocol).await.aynhow_result()?);
	ONCE_STATE.call_once(move || {
        unsafe {
            DB_POOL = MaybeUninit::new(db);
        };
    });
	Ok(())
}
pub fn db_pool()->Arc<DatabaseConnection>{
	Arc::clone(unsafe {
		DB_POOL.assume_init_ref()
	})
}
