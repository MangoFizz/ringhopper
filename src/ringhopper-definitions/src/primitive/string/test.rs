use super::*;

#[test]
fn parse_string32() {
    let valid: [u8; 32] = ['v' as u8, 'a' as u8, 'l' as u8, 'i' as u8, 'd' as u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let valid = String32::from_bytes_lossy(&valid);
    assert_eq!("valid", valid);
    assert_eq!("valid", valid.as_str());

    let valid_dirty: [u8; 32] = ['v' as u8, 'a' as u8, 'l' as u8, 'i' as u8, 'd' as u8, 0, 'o' as u8, 'h' as u8, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let valid_dirty = String32::from_bytes_lossy(&valid_dirty);
    assert_eq!("valid", valid_dirty);
    assert_eq!("valid", valid_dirty.as_str());
    assert_eq!(valid, valid_dirty);

    let invalid: [u8; 32] = ['v' as u8, 'a' as u8, 'l' as u8, 'i' as u8, 'd' as u8, 0x90, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let invalid = String32::from_bytes_lossy(&invalid);
    assert_eq!("valid_", invalid);
    assert_eq!("valid_", invalid.as_str());

    let long_string = "asfdkhljaesrfewragaewrkguieqw4i7w43qe5iy9oewsrayuoirewquigwre5gisgisaerfuhgewrq5oiuh453iop;juji;ls";
    assert_eq!(Err(Error::String32SizeLimitExceeded), String32::from_str(long_string));

    assert_eq!(valid, String32::from_str("valid").expect("should be ok"));
}
