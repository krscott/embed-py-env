use anyhow::{anyhow, bail};
use reqwest::Url;
use std::{
    env, fs,
    io::{self, Cursor},
    path::{Path, PathBuf},
};
use structopt::StructOpt;
use tokio::process::Command;

#[derive(Debug, StructOpt)]
struct Opt {
    /// Python version (e.g. 3.9.7)
    #[structopt(short, long)]
    py_version: Option<String>,

    /// Output directory
    #[structopt(parse(from_os_str), default_value = "pydist")]
    output_dir: PathBuf,

    /// Pip requirements file
    #[structopt(short, long, parse(from_os_str))]
    requirements: Option<PathBuf>,
}

const GET_PIP_URL: &str = "https://bootstrap.pypa.io/get-pip.py";

type PyVerTuple = (u16, u16, u16);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();
    let py_version = match opt.py_version {
        Some(ver) => python_version_from_str(&ver)?,
        None => default_python_version().await?,
    };

    let pip_path = opt.output_dir.join("Scripts").join("pip.exe");

    if !pip_path.is_file() {
        create_embedded_env(&py_version, &opt.output_dir).await?;
    } else {
        eprintln!("Using existing env: {:?}", opt.output_dir);
    }

    if let Some(reqs_path) = opt.requirements {
        install_requirements(&pip_path, &opt.output_dir, &reqs_path).await?;
    }

    eprintln!("Done!");
    Ok(())
}

async fn install_requirements(
    pip_path: &Path,
    dist_dir: &Path,
    reqs_path: &Path,
) -> anyhow::Result<()> {
    eprintln!("Installing pip requirements...");

    let path_var = dist_env_path(dist_dir);

    let out = Command::new(pip_path.to_string_lossy().as_ref())
        .env("PATH", &path_var)
        .arg("install")
        .arg("-r")
        .arg(reqs_path.to_string_lossy().as_ref())
        .output()
        .await?;

    eprintln!("pip stdout: {}", String::from_utf8_lossy(&out.stdout));
    eprintln!("pip stderr: {}", String::from_utf8_lossy(&out.stderr));

    if !out.status.success() {
        bail!("pip failed");
    }

    Ok(())
}

async fn create_embedded_env(py_version: &PyVerTuple, dist_dir: &Path) -> anyhow::Result<()> {
    let libs_dir_src = host_python_dir(&py_version)?;
    let get_pip_path = dist_dir.join("get-pip.py");

    fs::create_dir_all(&dist_dir)?;

    // Download embedded zip file
    eprintln!("Downloading zip file...");
    {
        let py_zip = reqwest::get(python_embed_zip_url(py_version)?)
            .await?
            .bytes()
            .await?;

        zip::ZipArchive::new(Cursor::new(py_zip))?.extract(dist_dir)?;
    }

    // Copy libs
    eprintln!("Copying libs...");
    {
        fs_extra::dir::copy(
            libs_dir_src,
            &dist_dir,
            &fs_extra::dir::CopyOptions {
                skip_exist: true,
                depth: 1,
                ..Default::default()
            },
        )?;
    }

    // Enable site
    eprintln!("Enabling import site...");
    {
        let pth_path = dist_dir.join(format!("python{}{}._pth", py_version.0, py_version.1));
        let contents = fs::read_to_string(&pth_path)?;
        let contents = contents.replace("#import site", "import site");
        fs::write(&pth_path, contents)?;
    }

    // Download get-pip.py
    eprintln!("Downloading get-pip...");
    {
        let bytes = reqwest::get(GET_PIP_URL).await?.bytes().await?;
        let mut content = Cursor::new(bytes);
        let mut file = fs::File::create(&get_pip_path)?;
        io::copy(&mut content, &mut file)?;
    }

    // Install pip
    eprintln!("Installing pip...");
    {
        let path_var = dist_env_path(dist_dir);
        let python_bin = dist_dir.join("python");

        let out = Command::new(python_bin.to_string_lossy().as_ref())
            .env("PATH", &path_var)
            .arg(get_pip_path.to_string_lossy().as_ref())
            .arg("--no-warn-script-location")
            .output()
            .await?;

        eprintln!("get-pip stdout: {}", String::from_utf8_lossy(&out.stdout));
        eprintln!("get-pip stderr: {}", String::from_utf8_lossy(&out.stderr));

        if !out.status.success() {
            bail!("get-pip failed");
        }

        // fs::remove_file(&get_pip_path)?;
    }

    Ok(())
}

fn dist_env_path(dist_dir: &Path) -> String {
    let scripts_dir = dist_dir.join("Scripts");
    format!(
        "{}:{}",
        dist_dir.to_string_lossy(),
        scripts_dir.to_string_lossy()
    )
}

async fn default_python_version() -> anyhow::Result<PyVerTuple> {
    let out = Command::new("python")
        .args(&[
            "-c",
            "import sys; print('.'.join(str(x) for x in sys.version_info[:3]))",
        ])
        .output()
        .await?;

    let ver_str = String::from_utf8_lossy(&out.stdout);

    python_version_from_str(&ver_str)
}

fn python_version_from_str(s: &str) -> anyhow::Result<PyVerTuple> {
    let tuple = match s
        .trim()
        .split('.')
        .into_iter()
        .collect::<Vec<_>>()
        .as_slice()
    {
        &[major, minor, micro] => (major.parse()?, minor.parse()?, micro.parse()?),
        _ => bail!("Version must be of the format: 1.2.3"),
    };

    Ok(tuple)
}

fn python_embed_zip_url(version: &PyVerTuple) -> anyhow::Result<Url> {
    Ok(Url::parse(&format!(
        "https://www.python.org/ftp/python/{0}.{1}.{2}/python-{0}.{1}.{2}-embed-amd64.zip",
        version.0, version.1, version.2
    ))?)
}

fn host_python_dir(version: &PyVerTuple) -> anyhow::Result<PathBuf> {
    let target_py = format!("Python{}{}", version.0, version.1);
    let env_path = env::var_os("PATH").ok_or_else(|| anyhow!("missing PATH env"))?;

    env::split_paths(&env_path)
        .into_iter()
        .filter_map(|dir| {
            dir.ends_with(&target_py)
                .then(|| dir.join("libs"))
                .filter(|dir| dir.is_dir())
        })
        .next()
        .ok_or_else(|| anyhow!("Could not find any {}/libs in PATH", target_py))
}
