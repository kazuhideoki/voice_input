// CI環境で実行可能なテストを示すマーカー
#[cfg(feature = "ci-test")]
pub const CI_TEST_MODE: bool = true;

#[cfg(not(feature = "ci-test"))]
pub const CI_TEST_MODE: bool = false;