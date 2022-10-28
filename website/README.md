---
home: true
title: Home
heroImage: /images/logo-text.svg
heroText: null
actions:
  - text: Get Started
    link: /book/getting-started.html
    type: primary
  - text: Introduction
    link: /book/
    type: secondary
features:
  - title: Simplicity First
    details: Minimal setup with markdown-centered project structure helps you focus on writing.
  - title: Vue-Powered
    details: Enjoy the dev experience of Vue, use Vue components in markdown, and develop custom themes with Vue.
  - title: Performant
    details: VuePress generates pre-rendered static HTML for each page, and runs as an SPA once a page is loaded.
  - title: Themes
    details: Providing a default theme out of the box. You can also choose a community theme or create your own one.
  - title: Plugins
    details: Flexible plugin API, allowing plugins to provide lots of plug-and-play features for your site. 
  - title: Bundlers
    details: Default bundler is Vite, while Webpack is also supported. Choose the one you like!
footer: MIT Licensed | Copyright © 2019-present Salvo Team
---

### As Easy as 1, 2, 3

<CodeGroup>
  <CodeGroupItem title="main.rs" active>
  
```rust
use salvo::prelude::*;
#[handler]
async fn hello_world(res: &mut Response) {
    res.render("Hello world!");
}
#[tokio::main]
async fn main() {
    let router = Router::new().get(hello_world);
    let acceptor = TcpListener::new("127.0.0.1:7878").bind().await;
    Server::new(acceptor).serve(router).await;
}
```

  </CodeGroupItem>
  <CodeGroupItem title="Cargo.toml">
  
```bash
# install in your project
npm install -D vuepress@next

# create a markdown file
echo '# Hello VuePress' > README.md

# start writing
npx vuepress dev

# build to static files
npx vuepress build
```

  </CodeGroupItem>
</CodeGroup>
