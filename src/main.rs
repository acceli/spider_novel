use reqwest; // 引入 reqwest 库
use tokio; // 引入 tokio 库，用于异步操作
use regex::Regex; // 引入正则表达式库
use encoding_rs::GBK; // 引入 GBK 编码库
use reqwest::header::HeaderMap; // 引入 HeaderMap 类型
use std::error::Error; // 引入 Error trait
use std::io::{self, Write}; // 引入 IO 库
use std::fs::OpenOptions; // 引入文件操作库

// 创建并配置 HTTP 客户端
fn create_client() -> Result<reqwest::Client, Box<dyn Error>> {
    let mut headers = HeaderMap::new();
    headers.insert("User-Agent", "Mozilla/5.0 ... Chrome/119.0.0.0 Safari/537.36".parse()?);
    Ok(reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .default_headers(headers)
        .build()?)
}

// 编码搜索关键字为 GBK
fn encode_gbk(search_key: &str) -> String {
    GBK.encode(search_key).0.iter()
        .map(|&b| format!("%{:02X}", b))
        .collect::<String>()
}

// 正则表达式匹配并提取信息
fn regex_match<'a>(pattern: &str, text: &'a str) -> Result<Option<regex::Captures<'a>>, Box<dyn Error>> {
    let re = Regex::new(pattern)?;
    Ok(re.captures(text))
}

// 异步搜索函数
async fn search(client: &reqwest::Client, search_key: &str) -> Result<(String, String, String), Box<dyn Error>> {
    let encoded_key = encode_gbk(search_key);
    let params = [("searchkey", encoded_key), ("searchtype", "all".to_string())];
    let response = client.post("https://cooolr.online/modules/article/search.php")
        .form(&params)
        .send().await?;
    let body = response.text().await?;

    let re_list_item = Regex::new(r"(<li>[\s\S]*?<a target=[\s\S]*?<img[\s\S]*?</li>)")?;
    let mut title = String::new();
    let mut author = String::new();
    let mut node_url = String::new();

    for cap in re_list_item.captures_iter(&body) {
        if let Some(caps) = regex_match(r#"<a target="_blank" href="(.*?)" class="imgbox">"#, &cap[0])? {
            node_url = caps[1].to_string()
                .replace(".htm", "/")
                .replace("www.69shuba.com", "cooolr.online");
        }
        if let Some(caps) = regex_match(r#"<h3><a target="_blank" href=".*?">(.*?)</a></h3>"#, &cap[0])? {
            title = caps[1].to_string()
                .replace(r#"<span class="hottext">"#, "")
                .replace("</span>", "");
        }
        if let Some(caps) = regex_match(r#"<div class="labelbox">[\s\S]*?<label>(.*?)</label>"#, &cap[0])? {
            author = caps[1].to_string()
                .replace(r#"<span class="hottext">"#, "")
                .replace("</span>", "");
        }
        break; // 一旦找到匹配项，跳出循环
    }

    Ok((title, author, node_url))
}

// 异步获取小说目录列表
async fn get_node_list(client: &reqwest::Client, node_url: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let response = client.get(node_url).send().await?;
    let body = response.text().await?;
    let re_node = Regex::new(r#"<li data-num="\d*?"><a href="(https://www.69shuba.com/txt/.*?)">.*?</a></li>"#)?;
    let mut node_urls: Vec<String> = Vec::new();
    for cap in re_node.captures_iter(&body) {
        if let Some(matched) = cap.get(1) {
            let node_url = matched.as_str().to_string().replace("www.69shuba.com", "cooolr.online");
            node_urls.push(node_url);
        }
    }
    Ok(node_urls)
}

// 异步获取小说内容
async fn get_content(client: &reqwest::Client, node_url: &str) -> Result<String, Box<dyn Error>> {
    let response = client.get(node_url).send().await?;
    let body_bytes = response.bytes().await?;
    let (cow, _, _) = GBK.decode(&body_bytes);
    let body = cow.into_owned();
    let re_content = Regex::new(r#"(?s)<script>loadAdv\(2,0\);</script>(.*?)<script>loadAdv\(3,0\);</script>"#)?;
    let mut content = String::new();
    if let Some(caps) = re_content.captures(&body) {
        content = caps[1].to_string()
            .replace("<div class=\"bottom-ad\">", "")
            .replace("</div>", "")
            .replace("&nbsp;", " ")
            .replace("<br />", "")
            .replace("<br>", "")
            .replace("\r", "")
            .replace("&emsp;", "  ")
            .replace("(本章完)", "");
    }
    Ok(content)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let client = create_client()?;
    print!("请输入小说名称：");
    io::stdout().flush()?; // 确保立即显示提示信息
    let mut search_key = String::new();
    io::stdin().read_line(&mut search_key)?;

    let search_key = search_key.trim(); // 现在这里是唯一一处对 search_key 的引用
    match search(&client, search_key).await {
        Ok((title, author, node_url)) => {
            println!("\n搜索命中！名称：{} 作者: {}\n\n回车确认下载，Ctrl C退出", title, author);
            let mut confirm = String::new();
            io::stdin().read_line(&mut confirm)?; // 使用新的变量进行确认输入

            let node_list = get_node_list(&client, &node_url).await?;
            let file_name = format!("{}.txt", title); // 使用不可变借用
            let mut file = OpenOptions::new()
                .append(true)
                .create(true)
                .open(&file_name)?;

            let total = node_list.len();
            for (index, url) in node_list.iter().enumerate() {
                let content = get_content(&client, url).await?;
                file.write_all(content.as_bytes())?;

                // 显示下载进度
                let progress = (index + 1) as f32 / total as f32 * 100.0;
                println!("进度: {:.2}%", progress);
            }

            println!("小说 '{}' 下载完成，回车退出！", file_name);
            io::stdin().read_line(&mut confirm)?; // 使用新的变量进行确认输入
            Ok(())
        },
        Err(e) => {
            println!("搜索出错：{}", e);
            Err(e)
        }
    }
}


