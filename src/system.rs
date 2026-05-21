use std::{io, path::Path, process::Command};

pub fn open_output_dir(path: &Path) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path).spawn()?;
    }

    #[cfg(not(target_os = "macos"))]
    {
        Command::new("xdg-open").arg(path).spawn()?;
    }

    Ok(())
}
