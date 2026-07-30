use std::path::Path;

use anyhow::{Context, Result};

pub fn slugify(s: &str) -> String {
    s.to_lowercase().replace(' ', "-").replace('.', "_")
}

pub fn create_random_subdomain() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..16].to_string()
}

pub fn compress_dir(dir: &str) -> Result<Vec<u8>> {
    let path = Path::new(dir);
    if !path.is_dir() {
        anyhow::bail!("Directory not found: {dir}");
    }

    let enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut tar = tar::Builder::new(enc);
    tar.follow_symlinks(true);
    tar.append_dir_all(".", path)
        .with_context(|| format!("Error archiving directory {dir}"))?;
    Ok(tar.into_inner()?.finish()?)
}

pub fn print_table(rows: &[Vec<String>]) {
    if rows.is_empty() {
        return;
    }

    let n_cols = rows.iter().map(|r| r.len()).max().unwrap();
    let mut widths = vec![0usize; n_cols];
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }

    for row in rows {
        let line = row
            .iter()
            .enumerate()
            .map(|(i, cell)| format!("{:<w$}", cell, w = widths[i] + 3))
            .collect::<String>();
        println!("{}", line.trim_end());
    }
}
