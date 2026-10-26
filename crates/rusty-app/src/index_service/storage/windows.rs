use std::io;
use std::path::PathBuf;

use windows_sys::Win32::UI::Shell::FOLDERID_ProgramData;

use super::config::Config;
use super::directory::{Policy, ProtectedDirectory, known_folder};
use super::install_plan::{NAVO, Product};

pub(super) struct Storage {
    pub program_data: PathBuf,
    pub directory: PathBuf,
    _guard: ProtectedDirectory,
}

impl Storage {
    pub fn open(create: bool) -> io::Result<Self> {
        Self::open_for(NAVO, create)
    }

    pub fn open_for(product: Product, create: bool) -> io::Result<Self> {
        let program_data = known_folder(&FOLDERID_ProgramData)?;
        let [parent, directory] = product.data_directories(&program_data);
        let guard = ProtectedDirectory::open(&[&parent, &directory], Policy::Data, create)?;
        Ok(Self {
            program_data,
            directory,
            _guard: guard,
        })
    }

    pub fn read_config(&self) -> io::Result<Config> {
        Config::decode(&std::fs::read(self.directory.join("service.json"))?)
    }

    pub fn write_config(&self, config: &Config) -> io::Result<()> {
        let bytes = serde_json::to_vec(config).map_err(io::Error::other)?;
        rusty_store::write_atomic(&self.directory.join("service.json"), &bytes)
    }
}
