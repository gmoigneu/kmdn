//! Blocking JSON client shared by the GitHub and GitLab providers: fixed headers, one error
//! shape, pagination, and an ETag cache so repeated GETs cost nothing on a 304 (D31).

use std::collections::HashMap;
use std::sync::Mutex;

use reqwest::blocking::{Client, RequestBuilder, Response};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, USER_AGENT};
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::{ProviderError, Result};

pub const UA: &str = "kmdn";

pub struct JsonClient {
    client: Client,
    base: String,
    headers: Vec<(HeaderName, String)>,
    etags: Mutex<HashMap<String, (String, String)>>,
}

impl JsonClient {
    /// `base` is the API root without a trailing slash; `headers` are sent on every request.
    pub fn new(base: &str, headers: Vec<(HeaderName, String)>) -> Self {
        Self {
            client: Client::new(),
            base: base.trim_end_matches('/').to_string(),
            headers,
            etags: Default::default(),
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    fn req(&self, rb: RequestBuilder) -> RequestBuilder {
        let mut map = HeaderMap::new();
        map.insert(USER_AGENT, HeaderValue::from_static(UA));
        map.insert(ACCEPT, HeaderValue::from_static("application/json"));
        for (k, v) in &self.headers {
            if let Ok(v) = HeaderValue::from_str(v) {
                map.insert(k.clone(), v);
            }
        }
        rb.headers(map)
    }

    /// Non-2xx becomes `ProviderError::Status` with the first 500 characters of the body.
    pub fn check(resp: Response) -> Result<Response> {
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        let url = resp.url().to_string();
        let body = resp.text().unwrap_or_default();
        Err(ProviderError::Status {
            status: status.as_u16(),
            url,
            body: body.chars().take(500).collect(),
        })
    }

    /// GET with `If-None-Match` when the URL was seen before; a 304 returns the cached body.
    pub fn get_text(&self, url: &str) -> Result<String> {
        let cached = self.etags.lock().ok().and_then(|m| m.get(url).cloned());
        let mut rb = self.req(self.client.get(url));
        if let Some((etag, _)) = &cached {
            rb = rb.header(reqwest::header::IF_NONE_MATCH, etag.clone());
        }
        let resp = rb.send()?;
        if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
            if let Some((_, body)) = cached {
                return Ok(body);
            }
        }
        let resp = Self::check(resp)?;
        let etag = resp
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let body = resp.text()?;
        if let (Some(etag), Ok(mut m)) = (etag, self.etags.lock()) {
            if m.len() > 512 {
                m.clear();
            }
            m.insert(url.to_string(), (etag, body.clone()));
        }
        Ok(body)
    }

    pub fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let body = self.get_text(&self.url(path))?;
        serde_json::from_str::<T>(&body).map_err(|e| ProviderError::Decode(e.to_string()))
    }

    fn send_json<T: DeserializeOwned>(&self, rb: RequestBuilder, body: &Value) -> Result<T> {
        let resp = Self::check(self.req(rb).json(body).send()?)?;
        resp.json::<T>()
            .map_err(|e| ProviderError::Decode(e.to_string()))
    }

    pub fn post<T: DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T> {
        self.send_json(self.client.post(self.url(path)), body)
    }

    /// POST to an absolute URL (GraphQL endpoints, OAuth hosts).
    pub fn post_absolute<T: DeserializeOwned>(&self, url: &str, body: &Value) -> Result<T> {
        self.send_json(self.client.post(url), body)
    }

    pub fn put<T: DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T> {
        self.send_json(self.client.put(self.url(path)), body)
    }

    pub fn patch<T: DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T> {
        self.send_json(self.client.patch(self.url(path)), body)
    }

    pub fn put_no_body(&self, path: &str, body: &Value) -> Result<()> {
        Self::check(
            self.req(self.client.put(self.url(path)))
                .json(body)
                .send()?,
        )?;
        Ok(())
    }

    /// POST a form to an absolute URL with no auth (OAuth device flow) and decode JSON.
    pub fn post_form_anon<T: DeserializeOwned>(url: &str, form: &[(&str, &str)]) -> Result<T> {
        let resp = Client::new()
            .post(url)
            .header(USER_AGENT, UA)
            .header(ACCEPT, "application/json")
            .form(form)
            .send()?;
        Self::check(resp)?
            .json::<T>()
            .map_err(|e| ProviderError::Decode(e.to_string()))
    }

    /// Follows `per_page=100&page=N` up to 20 pages.
    pub fn all_pages<T: DeserializeOwned>(&self, path: &str) -> Result<Vec<T>> {
        let mut out = Vec::new();
        for page in 1..=20 {
            let sep = if path.contains('?') { '&' } else { '?' };
            let items: Vec<T> = self.get(&format!("{path}{sep}per_page=100&page={page}"))?;
            let n = items.len();
            out.extend(items);
            if n < 100 {
                break;
            }
        }
        Ok(out)
    }
}

/// String field or empty.
pub(crate) fn s(v: &Value, k: &str) -> String {
    v.get(k)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Optional string field.
pub(crate) fn os(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}

pub(crate) fn urlencode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
}
