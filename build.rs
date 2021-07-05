use std::{borrow::Cow, env, error::Error, fmt, fs::{self, File, read_dir}, io::{self, BufWriter, Read, Write}, path::Path};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

struct Post {
    /// a post must be `src/posts/{filename}.md`, 
    /// where `filename` is the unique identifier of the post.
    /// ideally in snake_case
    pub filename: String
}

impl fmt::Display for Post {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, r####"#[doc=include_str!(p!(r###"/posts/{}.md"###))]"####, self.filename)
    }
}

enum ModuleContent {
    Post(Post)
}

impl fmt::Display for ModuleContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Post(p) => p.fmt(f)
        }
    }
}

struct Root(Vec<ModuleTree>);

#[derive(Default)]
struct ModuleTree {
    pub name: Cow<'static, str>,
    pub children: Vec<ModuleTree>,
    pub content: Option<ModuleContent>
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
                let (_cfg, content) = st.split_once("%%").unwrap();
                path.push(file_name.as_ref());
                fs::write(&path, content)?;
                path.pop();
                posts.push(Post { filename: id.to_string() })
            }
            _ => println!("cargo:warning=ignored: {}", file_name)
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