use crate::error::*;
use crate::*;

use cryptsetup_rs as luks;
use cryptsetup_rs::api::{CryptDeviceHandle, CryptDeviceOpenBuilder, Luks1Params};
use cryptsetup_rs::{Luks1CryptDevice, CryptDevice};
use ctap;
use ctap::extensions::hmac::{FidoHmacCredential, HmacExtension};
use ctap::FidoDevice;

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::thread;
use std::time::Duration;

pub fn setup() -> Fido2LuksResult<()> {
    while !authenticator_connected()? {
        eprintln!("Please connect your authenticator");
        for _ in 0..3 {
            thread::sleep(Duration::from_secs(1));
            if authenticator_connected()? {
                break;
            }
        }
    }

    let mut config = Config::default();

    let save_config = |c: &Config| {
        File::create("fido2luks.json")
            .expect("Failed to save config")
            .write_all(serde_json::to_string_pretty(c).unwrap().as_bytes())
            .expect("Failed to save config");
    };

    fn ask_bool(q: &str) -> bool {
        ask_str(&format!("{} (y/n)", q)).expect("Failed to read from stdin") == "y"
    }

    println!("1. Generating a credential");
    let mut ccred: Option<FidoHmacCredential> = None;
    for di in ctap::get_devices().expect("Failed to query USB for 2fa devices") {
        let mut dev = FidoDevice::new(&di).expect("Failed to open 2fa device");
        match dev.make_hmac_credential() {
            Ok(cred) => {
                ccred = Some(cred);
                break;
            }
            Err(_e) => println!("Failed to to obtain credential trying next device(if applicable)"),
        }
    }
    config.credential_id = hex::encode(ccred.expect("No credential could be obtained").id);
    save_config(&config);

    loop {
        let device = ask_str("Path to your luks device: ").expect("Failed to read from stdin");;
        if Path::new(&device).exists()
            || ask_bool(&format!("{} does not exist, save anyway?", device))
        {
            config.device = device.into();
            break;
        }
    }

    save_config(&config);

    config.mapper_name = ask_str("Name for decrypted disk: ").expect("Failed to read from stdin");;

    save_config(&config);

    println!("Config saved to: fido2luks.json");

    //let slot = add_key_to_luks(&config).expect("Failed to add key to device");

    //println!("Added key to slot: {}", slot);

    Ok(())
}

pub fn add_key_to_luks(device: PathBuf, secret: &[u8; 32]) -> Fido2LuksResult<u8> {
    fn offer_format(
        _dev: CryptDeviceOpenBuilder,
    ) -> Fido2LuksResult<CryptDeviceHandle<Luks1Params>> {
        unimplemented!()
    }
    let dev =
        || -> luks::device::Result<CryptDeviceOpenBuilder> { luks::open(&device.canonicalize()?) };

    let prev_key_info = rpassword::read_password_from_tty(Some(
        "Please enter your current password or path to a keyfile in order to add a new key: ",
    ))?;

    let prev_key = match prev_key_info.as_ref() {
        "" => None,
        keyfile if PathBuf::from(keyfile).exists() => {
            let mut f = File::open(keyfile)?;
            let mut key = Vec::new();
            f.read_to_end(&mut key)?;
            Some(key)
        }
        password => Some(Vec::from(password.as_bytes())),
    };

    let mut handle = match dev()?.luks1() {
        Ok(handle) => handle,
        Err(luks::device::Error::BlkidError(_)) => offer_format(dev()?)?,
        Err(luks::device::Error::CryptsetupError(errno)) => {
            //if i32::from(errno) == 555
            dbg!(errno);
            offer_format(dev()?)?
        } //TODO: find correct errorno and offer to format as luks
        err => err?,
    };
    handle.set_iteration_time(50);
    let slot = handle.add_keyslot(secret, prev_key.as_ref().map(|b| b.as_slice()), None)?;
    Ok(slot)
}

pub fn authenticator_connected() -> Fido2LuksResult<bool> {
    Ok(!device::get_devices()?.is_empty())
}
