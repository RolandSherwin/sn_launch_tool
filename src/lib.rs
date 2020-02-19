// Copyright 2020 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under the MIT license <LICENSE-MIT
// http://opensource.org/licenses/MIT> or the Modified BSD license <LICENSE-BSD
// https://opensource.org/licenses/BSD-3-Clause>, at your option. This file may not be copied,
// modified, or distributed except according to those terms. Please review the Licences for the
// specific language governing permissions and limitations relating to use of the SAFE Network
// Software.

use directories::BaseDirs;
use log::debug;
use regex::Regex;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::Duration,
};
use structopt::StructOpt;

#[cfg(not(target_os = "windows"))]
const SAFE_VAULT_EXECUTABLE: &str = "safe_vault";

#[cfg(target_os = "windows")]
const SAFE_VAULT_EXECUTABLE: &str = "safe_vault.exe";

/// Tool to launch SAFE vaults to form a local single-section network
///
/// Currently, this tool runs vaults on localhost (since that's the default if no IP address is given to the vaults)
#[derive(StructOpt, Debug)]
#[structopt(name = "safe-nlt")]
struct CmdArgs {
    /// Vebosity level for this tool
    #[structopt(short = "v", long, parse(from_occurrences))]
    verbosity: u8,

    /// Path where to locate safe_vault/safe_vault.exe binary. The SAFE_VAULT_PATH env var can be also used to set the path
    #[structopt(short = "p", long, env = "SAFE_VAULT_PATH")]
    vault_path: Option<PathBuf>,

    /// Interval in seconds between launching each of the vaults
    #[structopt(short = "i", long, default_value = "5")]
    interval: u64,

    /// Path where the output directories for all the vaults are written
    #[structopt(short = "d", long, default_value = "./vaults")]
    vaults_dir: PathBuf,

    /// Number of vaults to spawn with the first one being the genesis. This number should be greater than 0.
    #[structopt(short = "n", long, default_value = "8")]
    num_vaults: u8,

    /// Vebosity level for vaults logs
    #[structopt(short = "y", long, parse(from_occurrences))]
    vaults_verbosity: u8,
}

pub fn run() -> Result<(), String> {
    run_with(None)
}

pub fn run_with(cmd_args: Option<&[&str]>) -> Result<(), String> {
    // Let's first get all the arguments passed in, either as function's args, or CLI args
    let args = match cmd_args {
        None => CmdArgs::from_args(),
        Some(cmd_args) => CmdArgs::from_iter_safe(cmd_args).map_err(|err| err.to_string())?,
    };

    let vault_bin_path = get_vault_bin_path(args.vault_path)?;
    let msg = format!(
        "Launching with vault executable from: {}",
        vault_bin_path.display()
    );
    if args.verbosity > 0 {
        println!("{}", msg);
    }
    debug!("{}", msg);

    let msg = format!("Network size: {} vaults", args.num_vaults);
    if args.verbosity > 0 {
        println!("{}", msg);
    }
    debug!("{}", msg);

    let mut common_args: Vec<&str> = vec![];

    let mut verbosity = String::from("-");
    if args.vaults_verbosity > 0 {
        for _ in 0..args.vaults_verbosity {
            verbosity.push('v');
        }
        common_args.push(&verbosity);
    }

    // Construct genesis vault's command arguments
    let mut genesis_vault_args = common_args.clone();
    genesis_vault_args.push("--first");
    let genesis_vault_dir = &args.vaults_dir.join("safe-vault-genesis");
    let genesis_vault_dir_str = genesis_vault_dir.display().to_string();
    genesis_vault_args.push("--root-dir");
    genesis_vault_args.push(&genesis_vault_dir_str);
    genesis_vault_args.push("--log-dir");
    genesis_vault_args.push(&genesis_vault_dir_str);

    // Let's launch genesis vault now
    let msg = "Launching genesis vault (#1)...";
    if args.verbosity > 0 {
        println!("{}", msg);
    }
    debug!("{}", msg);
    run_vault_cmd(&vault_bin_path, &genesis_vault_args, args.verbosity)?;

    // Get port number of genesis vault to pass it as hard-coded contact to the other vaults
    let interval_duration = Duration::from_secs(args.interval);
    thread::sleep(interval_duration);
    let genesis_contant_info = grep_connection_info(&genesis_vault_dir.join("safe_vault.log"))?;
    let msg = format!("Genesis vault contact info: {}", genesis_contant_info);
    if args.verbosity > 0 {
        println!("{}", msg);
    }
    debug!("{}", msg);

    // We can now run the rest of the vaults
    for i in 2..args.num_vaults + 1 {
        // Construct current vault's command arguments
        let mut current_vault_args = common_args.clone();
        let vault_dir = &args
            .vaults_dir
            .join(&format!("safe-vault-{}", i))
            .display()
            .to_string();

        current_vault_args.push("--root-dir");
        current_vault_args.push(vault_dir);
        current_vault_args.push("--log-dir");
        current_vault_args.push(vault_dir);
        current_vault_args.push("--hard-coded-contacts");
        current_vault_args.push(&genesis_contant_info);

        let msg = format!("Launching vault #{}...", i);
        if args.verbosity > 0 {
            println!("{}", msg);
        }
        debug!("{}", msg);
        run_vault_cmd(&vault_bin_path, &current_vault_args, args.verbosity)?;

        // We wait for a few secs before launching each new vault
        thread::sleep(interval_duration);
    }

    println!("Done!");
    Ok(())
}

#[inline]
fn get_vault_bin_path(vault_path: Option<PathBuf>) -> Result<PathBuf, String> {
    match vault_path {
        Some(p) => Ok(p),
        None => {
            let base_dirs =
                BaseDirs::new().ok_or_else(|| "Failed to obtain user's home path".to_string())?;

            let mut path = PathBuf::from(base_dirs.home_dir());
            path.push(".safe");
            path.push("vault");
            path.push(SAFE_VAULT_EXECUTABLE);
            Ok(path)
        }
    }
}

fn run_vault_cmd(vault_path: &PathBuf, args: &[&str], verbosity: u8) -> Result<(), String> {
    let path_str = vault_path.display().to_string();
    let msg = format!("Running '{}' with args {:?} ...", path_str, args);
    if verbosity > 1 {
        println!("{}", msg);
    }
    debug!("{}", msg);

    let _child = Command::new(&path_str)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| {
            format!(
                "Failed to run '{}' with args '{:?}': {}",
                path_str, args, err
            )
        })?;

    Ok(())
}

fn grep_connection_info(log_path: &PathBuf) -> Result<String, String> {
    let regex_query = Regex::new(r".+Vault connection info:\s(.+)$").map_err(|err| {
        format!(
            "Failed to obtain the contact info of the genesis vault: {}",
            err
        )
    })?;
    let file_content = fs::read_to_string(log_path).map_err(|err| {
        format!(
            "Failed to obtain the contact info of the genesis vault: {}",
            err
        )
    })?;

    for (_, line) in file_content.lines().enumerate() {
        if let Some(contact_info) = &regex_query.captures(&line) {
            return Ok(format!("[{}]", contact_info[1].to_string()));
        }
    }

    Err("Failed to find the contact info of the genesis vault".to_string())
}