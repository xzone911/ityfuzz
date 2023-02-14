use glob::glob;
use serde_json::Value;
use std::collections::HashMap;

use std::fs::File;

use std::io::Read;
use std::path::Path;
use primitive_types::H160;

extern crate crypto;

use crate::abi::get_abi_type_boxed_with_address;
use crate::rand_utils::{generate_random_address, fixed_address};

use self::crypto::digest::Digest;
use self::crypto::sha3::Sha3;

// to use this address, call rand_utils::fixed_address(FIX_DEPLOYER)
pub static FIX_DEPLOYER: &str = "8b21e662154b4bbc1ec0754d0238875fe3d22fa6";

#[derive(Debug, Clone)]
pub struct ABIConfig {
    pub abi: String,
    pub function: [u8; 4],
    pub is_static: bool,
    pub is_constructor: bool,
}

#[derive(Debug, Clone)]
pub struct ContractInfo {
    pub name: String,
    pub abi: Vec<ABIConfig>,
    pub code: Vec<u8>,
    pub constructor_args: Vec<u8>,
    pub deployed_address: H160,
}

#[derive(Debug, Clone)]
pub struct ContractLoader {
    pub contracts: Vec<ContractInfo>,
}

pub fn set_hash(name: &str, out: &mut [u8]) {
    let mut hasher = Sha3::keccak256();
    hasher.input_str(name);
    hasher.result(out)
}

impl ContractLoader {
    fn parse_abi(path: &Path) -> Vec<ABIConfig> {
        let mut file = File::open(path).unwrap();
        let mut data = String::new();
        file.read_to_string(&mut data)
            .expect("failed to read abi file");
        let json: Vec<Value> = serde_json::from_str(&data).expect("failed to parse abi file");
        json.iter()
            .flat_map(|abi| {
                if abi["type"] == "function" || abi["type"] == "constructor" {
                    let name = if abi["type"] == "function" {
                        abi["name"].as_str().expect("failed to parse abi name")
                    } else {
                        "constructor"
                    };
                    let mut abi_name: Vec<String> = vec![];
                    abi["inputs"]
                        .as_array()
                        .expect("failed to parse abi inputs")
                        .iter()
                        .for_each(|input| {
                            abi_name.push(input["type"].as_str().unwrap().to_string());
                        });
                    let mut abi_config = ABIConfig {
                        abi: format!("({})", abi_name.join(",")),
                        function: [0; 4],
                        is_static: abi["stateMutability"].as_str().unwrap() == "view",
                        is_constructor: abi["type"] == "constructor",
                    };
                    let function_to_hash = format!("{}({})", name, abi_name.join(","));
                    // print name and abi_name
                    println!("{}({})", name, abi_name.join(","));

                    set_hash(function_to_hash.as_str(), &mut abi_config.function);
                    Some(abi_config)
                } else {
                    None
                }
            })
            .collect()
    }

    fn parse_hex_file(path: &Path) -> Vec<u8> {
        let mut file = File::open(path).unwrap();
        let mut data = String::new();
        file.read_to_string(&mut data).unwrap();
        hex::decode(data).expect("Failed to parse hex file")
    }

    pub fn from_prefix(prefix: &str) -> Self {
        let mut result = ContractInfo {
            name: prefix.to_string(),
            abi: vec![],
            code: vec![],
            constructor_args: vec![], // todo: fill this
            deployed_address: generate_random_address()
        };
        println!("Loading contract {}", prefix);
        for i in glob(prefix).expect("not such path for prefix") {
            match i {
                Ok(path) => {
                    if path.to_str().unwrap().ends_with(".abi") {
                        // this is an ABI file
                        result.abi = Self::parse_abi(&path);
                        // println!("ABI: {:?}", result.abi);
                    } else if path.to_str().unwrap().ends_with(".bin") {
                        // this is an BIN file
                        result.code = Self::parse_hex_file(&path);
                    } else if path.to_str().unwrap().ends_with(".address") {
                        // this is deployed address
                        result.deployed_address.0.clone_from_slice(Self::parse_hex_file(&path).as_slice());
                    } else {
                        println!("Found unknown file: {:?}", path.display())
                    }
                }
                Err(e) => println!("{:?}", e),
            }
        }

        if let Some(abi) = result.abi.iter().find(|abi| abi.is_constructor) {
            let mut abi_instance = get_abi_type_boxed_with_address(&abi.abi, fixed_address(FIX_DEPLOYER).0.to_vec());
            abi_instance.set_func(abi.function);
            // since this is constructor args, we ingore the function hash
            // Note (Shangyin): this may still non-deployable, need futher improvement
            // (The check may fail)

            let mut random_bytes = vec![0u8; abi_instance.get().get_bytes().len()];
            for i in 0..random_bytes.len() {
                random_bytes[i] = rand::random();
            }
            print!("Random bytes {:?}", random_bytes);
            // result.constructor_args = random_bytes;
            result.constructor_args = abi_instance.get().get_bytes();
            // println!("Constructor args: {:?}", result.constructor_args);
            result.code.extend(result.constructor_args.clone());
        }
        return Self {
            contracts: if result.code.len() > 0 {
                vec![result]
            } else {
                vec![]
            },
        };
    }

    // This function loads constructs Contract infos from path p
    // The organization of directory p should be
    // p
    // |- contract1.abi
    // |- contract1.bin
    // |- contract2.abi
    // |- contract2.bin
    pub fn from_glob(p: &str) -> Self {
        let mut prefix_file_count: HashMap<String, u8> = HashMap::new();
        for i in glob(p).expect("not such folder") {
            match i {
                Ok(path) => {
                    let path_str = path.to_str().unwrap();
                    if path_str.ends_with(".abi") {
                        *prefix_file_count
                            .entry(path_str.replace(".abi", "").clone())
                            .or_insert(0) += 1;
                    } else if path_str.ends_with(".bin") {
                        *prefix_file_count
                            .entry(path_str.replace(".bin", "").clone())
                            .or_insert(0) += 1;
                    } else {
                        println!("Found unknown file in folder: {:?}", path.display())
                    }
                }
                Err(e) => println!("{:?}", e),
            }
        }

        let mut contracts: Vec<ContractInfo> = vec![];
        for (prefix, count) in prefix_file_count {
            if count == 2 {
                for contract in
                    Self::from_prefix((prefix.to_owned() + &String::from('*')).as_str()).contracts
                {
                    contracts.push(contract);
                }
            }
        }

        ContractLoader { contracts }
    }

    pub fn from_glob_target(p: &str, target: &str) -> Self {
        let prefix = Path::new(p).join(target);
        let abi_path = prefix.with_extension("abi");
        let bin_path = prefix.with_extension("bin");

        if !(abi_path.exists() && bin_path.exists()) {
            panic!("ABI or BIN file not found for {}", target);
        }

        Self::from_prefix((prefix.to_str().unwrap().to_owned() + &String::from('*')).as_str())
    }
}

mod tests {
    use super::*;

    #[test]
    fn test_load() {
        let loader = ContractLoader::from_glob("demo/*");
        println!(
            "{:?}",
            loader
                .contracts
                .iter()
                .map(|x| x.name.clone())
                .collect::<Vec<String>>()
        );
    }
}
