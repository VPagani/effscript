//! `efs init`: scaffolds a project.

use std::path::Path;

const PACKAGE_JSON: &str = r#"{
  "name": "NAME",
  "private": true,
  "type": "module",
  "scripts": {
    "build": "efs build && tsc",
    "check": "efs check",
    "fmt": "efs fmt",
    "lint": "efs lint"
  },
  "dependencies": {
    "effect": "^4.1.6"
  },
  "devDependencies": {
    "typescript": "^7.1.0"
  }
}
"#;

const TSCONFIG: &str = r#"{
  "compilerOptions": {
    "strict": true,
    "exactOptionalPropertyTypes": true,
    "target": "es2022",
    "module": "nodenext",
    "moduleResolution": "nodenext",
    "lib": ["es2022"],
    "skipLibCheck": true,
    "rootDir": "src",
    "outDir": "dist"
  },
  "include": ["src"]
}
"#;

const MAIN: &str = r#"error EmptyName

effect function greet(name: string) {
  if name.length == 0 {
    fail new EmptyName();
  }
  run Console.log(`Hello, ${name}!`);
}

export const main = greet("Effect")
  catch {
    EmptyName => Console.log("Who are you?")
  };

Effect.runPromise(main);
"#;

const EFS_JSON: &str = r#"{
  "importExtension": ".js",
  "sourceMap": true
}
"#;

const GITIGNORE: &str = "node_modules/\ndist/\nsrc/**/*.ts\nsrc/**/*.ts.map\n";

pub fn init(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir.join("src")).map_err(|e| e.to_string())?;
    let name = std::fs::canonicalize(dir)
        .ok()
        .and_then(|d| d.file_name().map(|n| n.to_string_lossy().to_lowercase()))
        .unwrap_or_else(|| "efs-app".into());
    let files = [
        ("package.json", PACKAGE_JSON.replace("NAME", &name)),
        ("tsconfig.json", TSCONFIG.to_string()),
        ("efs.json", EFS_JSON.to_string()),
        (".gitignore", GITIGNORE.to_string()),
        ("src/main.efs", MAIN.to_string()),
    ];
    for (path, _) in &files {
        if dir.join(path).exists() {
            return Err(format!("{} already exists; not overwriting anything", dir.join(path).display()));
        }
    }
    for (path, content) in files {
        std::fs::write(dir.join(path), content).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn template_sources_are_formatted() {
        assert_eq!(efs_fmt::format(super::MAIN).unwrap(), super::MAIN);
    }
}
