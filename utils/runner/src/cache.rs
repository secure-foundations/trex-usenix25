use anyhow::Context;

use crate::{glob, job::JobType};
use std::{
    collections::HashMap,
    io::{Read, Write},
    os::unix::prelude::OsStrExt,
    path::PathBuf,
    time::Duration,
};

pub struct Cache {
    path: PathBuf,
}

#[derive(Debug)]
pub enum CacheEntry {
    Dir,
    File(PathBuf),
}

impl Cache {
    const CACHE_DIR: &'static str = "./.runner-cache";

    pub fn new(job_typ: &JobType) -> Self {
        assert!(!job_typ.to_string().contains("/"));
        let path = std::path::Path::new(Self::CACHE_DIR).join(job_typ.to_string());
        if !path.exists() {
            std::fs::create_dir_all(&path).unwrap();
        }

        Self { path }
    }

    fn get_dep_hash(&self, dependencies: &[&str]) -> String {
        let dependencies: Vec<PathBuf> = {
            let mut t: Vec<_> = dependencies
                .into_iter()
                .map(|x| {
                    let r = glob(x);
                    assert!(!r.is_empty());
                    r
                })
                .flatten()
                .collect();
            t.sort();
            t
        };

        let mut hasher = blake3::Hasher::new();
        for dep in &dependencies {
            hasher.update(dep.as_os_str().as_bytes());
        }
        hasher.update(b"data:");
        for dep in &dependencies {
            assert!(dep.exists());
            hasher.update(dep.as_os_str().as_bytes());
            let mut f = std::fs::File::open(dep).unwrap();
            let mut buf = vec![];
            f.read_to_end(&mut buf).unwrap();
            hasher.update(&buf);
        }

        hasher.finalize().to_hex().to_string()
    }

    fn get_inp_hash(&self, inputs: &[PathBuf]) -> String {
        let inputs: Vec<PathBuf> = {
            let mut t: Vec<PathBuf> = inputs.iter().cloned().collect();
            t.sort();
            t
        };

        let mut hasher = blake3::Hasher::new();
        for inp in &inputs {
            hasher.update(inp.as_os_str().as_bytes());
        }
        hasher.update(b"data:");
        for inp in &inputs {
            assert!(inp.exists());
            hasher.update(inp.as_os_str().as_bytes());
            if inp.is_dir() {
                hasher.update(b"dir");
            } else {
                let mut f = std::fs::File::open(inp).unwrap();
                let mut buf = vec![];
                f.read_to_end(&mut buf).unwrap();
                hasher.update(&buf);
            }
        }

        hasher.finalize().to_hex().to_string()
    }

    fn dir_for(&self, inputs: &[PathBuf], dependencies: &[&str]) -> PathBuf {
        self.path
            .join(self.get_dep_hash(dependencies))
            .join(self.get_inp_hash(inputs))
    }

    const JOB_RUN_TIME: &'static str = "job-run-time";
    const NO_TIME_KNOWN: &'static str = "!!! NO TIME KNOWN !!!";

    pub fn get(&self, inputs: &[PathBuf], dependencies: &[&str]) -> HashMap<PathBuf, CacheEntry> {
        let dir = self.dir_for(inputs, dependencies);
        if !dir.exists() {
            return Default::default();
        }

        // Force bust any cache that doesn't have timing information known
        if !dir.join(Self::JOB_RUN_TIME).exists() {
            return Default::default();
        }

        let mut res = HashMap::new();
        let files: Vec<PathBuf> = glob(&format!("{}/**/*", dir.display()));
        for file in files {
            if file.is_dir() {
                res.insert(file.iter().skip(4).collect(), CacheEntry::Dir);
            } else {
                res.insert(file.iter().skip(4).collect(), CacheEntry::File(file));
            }
        }
        res
    }

    pub fn insert(
        &mut self,
        inputs: &[PathBuf],
        dependencies: &[&str],
        outputs: &[PathBuf],
        time_taken_for_job: Option<std::time::Duration>,
    ) -> anyhow::Result<()> {
        let dir = self.dir_for(inputs, dependencies);

        if !dir.exists() {
            std::fs::create_dir_all(&dir)
                .with_context(|| format!("Creating dir {}", dir.display()))?;
        }

        let job_run_time = match time_taken_for_job {
            Some(time) => format!("{}", time.as_secs_f64()),
            None => Self::NO_TIME_KNOWN.into(),
        };
        std::fs::write(dir.join(Self::JOB_RUN_TIME), job_run_time)
            .with_context(|| format!("Writing job-run-time"))?;

        for output in outputs {
            if output.is_dir() {
                let path = dir.join(&output);
                if !path.exists() {
                    std::fs::create_dir_all(&path)
                        .with_context(|| format!("Creating dir {}", path.display()))?;
                }
            } else {
                let path = dir.join(&output);
                let dir = path.parent().unwrap();
                if !dir.exists() {
                    std::fs::create_dir_all(dir)
                        .with_context(|| format!("Creating dir {}", dir.display()))?;
                }
                let mut f = std::fs::File::open(&output)
                    .with_context(|| format!("Opening {}", output.display()))?;
                let mut buf = vec![];
                f.read_to_end(&mut buf).unwrap();
                let mut f = std::fs::File::create(&path)
                    .with_context(|| format!("Creating {}", path.display()))?;
                f.write_all(&buf).unwrap();
            }
        }

        Ok(())
    }

    pub fn get_runtime(cached: &HashMap<PathBuf, CacheEntry>) -> Option<Duration> {
        match cached.get(&PathBuf::from(Self::JOB_RUN_TIME))? {
            CacheEntry::Dir => None,
            CacheEntry::File(path) => {
                let job_run_time = std::fs::read_to_string(path).unwrap();
                if job_run_time.trim() == Self::NO_TIME_KNOWN {
                    None
                } else {
                    Some(Duration::from_secs_f64(
                        job_run_time.parse::<f64>().unwrap(),
                    ))
                }
            }
        }
    }
}
