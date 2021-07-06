use chrono::naive::NaiveDate;
use std::{
    borrow::Cow,
    env,
    error::Error,
    fmt,
    fs::{self, read_dir, File},
    io::{self, BufWriter, Read, Write},
    path::Path,
    str::FromStr,
};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

struct PostCfg {
    pub author: String,
    pub published: NaiveDate,
    pub title: String,
    pub edited: Option<NaiveDate>,
}

fn parse_naive_date(s: &str) -> Result<NaiveDate> {
    Ok(NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|e| format!("Expected ISO 8601 Date: {}", e))?)
}

impl FromStr for PostCfg {
    type Err = Box<dyn Error>;

    fn from_str(s: &str) -> Result<Self> {
        let mut author = None;
        let mut title = None;
        let mut published = None;
        let mut edited = None;
        for l in s.lines() {
            let l = l.trim();
            if l.is_empty() {
                continue
            }
            let (key, value) = l.split_once("=").ok_or("expected key=value pair")?;
            let (key, value) = (key.trim(), value.trim());
            match key {
                "author" => author = Some(value.to_owned()),
                "title" => title = Some(value.to_owned()),
                "published" => published = Some(parse_naive_date(value)?),
                "edited" => edited = Some(parse_naive_date(value)?),
                _ => {}
            }
        }
        Ok(PostCfg {
            author: author.ok_or("Expected author")?,
            title: title.ok_or("Expected title")?,
            published: published.ok_or("Expected published date")?,
            edited,
        })
    }
}

struct Post {
    /// a post must be `src/posts/{filename}.md`,
    /// where `filename` is the unique identifier of the post.
    /// ideally in snake_case
    pub filename: String,
}

impl fmt::Display for Post {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            r####"#[doc=include_str!(p!(r###"/posts/{}.md"###))]"####,
            self.filename
        )
    }
}

enum ModuleContent {
    Post(Post),
}

impl fmt::Display for ModuleContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Post(p) => p.fmt(f),
        }
    }
}

struct Root(Vec<ModuleTree>);

#[derive(Default)]
struct ModuleTree {
    pub name: Cow<'static, str>,
    pub children: Vec<ModuleTree>,
    pub content: Option<ModuleContent>,
}

fn build_module_tree(out_dir: &Path) -> Result<Root> {
    let mut modules = vec![];
    let mut posts: Vec<Post> = vec![];
    let mut path = out_dir.join("posts/");
    fs::create_dir_all(&path)?;
    for entry in read_dir("src/posts")? {
        let entry = entry?;
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        match file_name.strip_suffix(".md") {
            Some(id) if entry.file_type()?.is_file() => {
                let mut st = String::new();
                File::open(entry.path())?.read_to_string(&mut st)?;
                let (cfg, content) = st.split_once("%%").unwrap();
                let PostCfg {
                    author,
                    title,
                    published,
                    edited,
                } = cfg.parse()?;
                path.push(file_name.as_ref());
                let mut file = File::create(&path)?;
                writeln!(file, "# {}\n", title)?;
                writeln!(file, "_By {} on {}_\n", author, published.format("%Y-%m-%d"))?;
                if let Some(edited) = edited {
                    writeln!(file, "Last Edited: {}\n", edited.format("%Y-%m-%d"))?;
                }
                file.write_all(content.as_bytes())?;
                path.pop();
                posts.push(Post {
                    filename: id.to_string(),
                })
            }
            _ => println!("cargo:warning=ignored: {}", file_name),
        }
    }
    let mut posts_mod = ModuleTree {
        name: "all_posts".into(),
        ..Default::default()
    };
    for post in posts {
        posts_mod.children.push(ModuleTree {
            name: post.filename.clone().into(),
            content: Some(ModuleContent::Post(post)),
            ..Default::default()
        })
    }
    modules.push(posts_mod);
    Ok(Root(modules))
}

fn print(rt: Root, dest: &Path) -> Result {
    let mut w = BufWriter::new(File::create(dest)?);
    w.write_all(br#"macro_rules! p { ($a:tt) => { concat!(env!("OUT_DIR"), $a) } }"#)?;
    fn print_inner<W: io::Write>(module: ModuleTree, w: &mut W) -> Result {
        if let Some(content) = module.content {
            write!(w, "{}", content)?;
        }
        write!(w, "pub mod {}{{", module.name)?;
        for child in module.children {
            print_inner(child, w)?;
        }
        write!(w, "}}")?;
        Ok(())
    }
    for m in rt.0 {
        print_inner(m, &mut w)?;
    }
    w.flush()?;
    Ok(())
}

fn main() -> Result {
    let out_dir = env::var("OUT_DIR")?;
    let root = build_module_tree(Path::new(&out_dir))?;
    let dest_path = Path::new(&out_dir).join("magic.rs");
    print(root, &dest_path)?;
    println!("cargo:rerun-if-changed=src/posts");
    Ok(())
}
