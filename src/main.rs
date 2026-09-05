use anyhow::{Context, Result};
use chrono::Local;
use dialoguer::MultiSelect;
use omniscient::modules::{self, AuditModule};
use omniscient::{health, hud, palette, pathcheck, report};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn print_header() {
    let p = palette::hot_pink();
    let c = palette::cyan();
    let w = palette::white();
    let purple = palette::purple();
    let reset = palette::RESET;
    let bold = palette::BOLD;

    print!("\x1b[2J\x1b[H"); // clear screen, matches fish's `clear`

    println!("{p}{bold}");
    println!("      _______________ ");
    println!("     /              \\");
    println!("    |  {c} _   _  {p}  |");
    println!("    | {c} ( )_( ) {p}  |");
    println!("    |  {c} \\_v_/  {p}  |");
    println!("     \\      __      /");
    println!("      \\    /  \\    /");
    println!("      \\  |    |  /");
    println!("       \\ |    | /");
    println!("        \\|____|/");
    println!("{w}{bold}  OMNISCIENT CYBERDECK v3.0{reset}");
    println!("{purple}  FULL SYSTEM REALITY SCANNER{reset}\n");
}

fn print_capability_matrix(mods: &[Box<dyn AuditModule>]) {
    let y = "\x1b[38;5;226m"; // keep the yellow heading distinct for scannability
    println!("{y} SYSTEM CAPABILITY MATRIX{}", palette::RESET);

    let mut seen = std::collections::HashSet::new();
    for m in mods {
        for tool in m.tools() {
            if !seen.insert(*tool) {
                continue;
            }
            if pathcheck::exists(tool) {
                println!("{} ✔ {tool}{}", palette::acid_green(), palette::RESET);
            } else {
                println!("{} ✖ {tool} MISSING{}", palette::red(), palette::RESET);
            }
        }
    }
    println!();
}

fn ensure_sudo() -> Result<()> {
    // One prompt upfront (validates and caches credentials), instead
    // of the fish version's scattered per-module sudo prompts that
    // interrupted the scan animation repeatedly.
    let status = Command::new("sudo")
        .arg("-v")
        .status()
        .context("running sudo -v")?;
    if !status.success() {
        anyhow::bail!("sudo authentication failed or was declined");
    }
    Ok(())
}

fn run_module(m: &dyn AuditModule, base_dir: &PathBuf, timestamp: &str) -> Result<(String, String)> {
    hud::scan(m.slug());
    let dir = base_dir.join(format!("{}-{}", m.slug(), timestamp));
    fs::create_dir_all(&dir)?;
    m.run(&dir)?;
    let rel = format!("{}-{}/{}.md", m.slug(), timestamp, m.slug());
    Ok((m.name().to_string(), rel))
}

fn main() -> Result<()> {
    let quiet = std::env::args().any(|a| a == "-q" || a == "--quiet");

    let base_dir = dirs_home().join(".arch-sys/system/omniscient");
    let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();

    let all = modules::all_modules();

    if !quiet {
        print_header();
        print_capability_matrix(&all);
    }

    let mut labels: Vec<&str> = vec!["👁️  Full System Audit"];
    labels.extend(all.iter().map(|m| m.menu_label()));

    let selections = MultiSelect::new()
        .with_prompt("CYBERDECK MODULE SELECTOR")
        .items(&labels)
        .interact_opt()
        .context("running module selector")?;

    let Some(selections) = selections else {
        println!("{} CANCELLED{}", palette::red(), palette::RESET);
        return Ok(());
    };
    if selections.is_empty() {
        println!("{} CANCELLED{}", palette::red(), palette::RESET);
        return Ok(());
    }

    let health = health::compute(&all);
    if !quiet {
        println!(
            "\n{} SYSTEM HEALTH SCORE: {}/100{}\n",
            palette::purple(),
            health.score,
            palette::RESET
        );
    }

    ensure_sudo()?;

    let full_selected = selections.contains(&0);
    let mut run_reports: Vec<(String, String)> = Vec::new();

    if full_selected {
        let master_dir = base_dir.join(format!("full_system_audit-{timestamp}"));
        fs::create_dir_all(&master_dir)?;
        for m in &all {
            let (name, rel) = run_module(m.as_ref(), &master_dir, &timestamp)?;
            run_reports.push((name, rel));
        }
        report::write_summary(
            &master_dir,
            &timestamp,
            &health,
            &run_reports
                .iter()
                .map(|(n, r)| (n.as_str(), r.as_str()))
                .collect::<Vec<_>>(),
        )?;
        println!("{} FULL SYSTEM AUDIT COMPLETE{}", palette::acid_green(), palette::RESET);
    } else {
        fs::create_dir_all(&base_dir)?;
        for &idx in &selections {
            let m = &all[idx - 1]; // -1: index 0 in `labels` is "Full System Audit"
            let (name, rel) = run_module(m.as_ref(), &base_dir, &timestamp)?;
            run_reports.push((name, rel));
        }
        report::write_summary(
            &base_dir,
            &timestamp,
            &health,
            &run_reports
                .iter()
                .map(|(n, r)| (n.as_str(), r.as_str()))
                .collect::<Vec<_>>(),
        )?;
    }

    if !quiet {
        println!("\n{} ▄▀▀▀▄▄▄▄▄▄▄▀▀▀▄ {}", palette::hot_pink(), palette::RESET);
        println!("{} --- CYBERDECK OFFLINE ---{}", palette::cyan(), palette::RESET);
    }

    Ok(())
}

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}