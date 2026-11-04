// Copyright 2025 TiDB Cloud
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// Use the standard jar so Domain, Path, Secure and expiry rules apply to every request.
pub(crate) use reqwest::cookie::Jar as GlobalCookieStore;

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::cookie::CookieStore;
    use reqwest::header::HeaderValue;
    use url::Url;

    #[test]
    fn cookies_are_scoped_to_their_destination() {
        let jar = GlobalCookieStore::default();
        let source = Url::parse("https://lake.example/private/page").unwrap();
        let cookie = HeaderValue::from_static("session=canary; Secure; Path=/private");
        jar.set_cookies(&mut [&cookie].into_iter(), &source);
        assert!(jar.cookies(&source).is_some());
        for url in [
            "https://other.example/private/page",
            "https://lake.example/public",
            "http://lake.example/private/page",
        ] {
            assert!(jar.cookies(&Url::parse(url).unwrap()).is_none(), "{url}");
        }
    }
}
