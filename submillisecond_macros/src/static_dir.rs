use std::{fs, io, path::Path};

use mime_guess::{mime, Mime};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    LitStr,
};

#[derive(Debug)]
pub struct StaticDir {
    files: Vec<StaticFile>,
}

impl StaticDir {
    pub fn expand(&self) -> TokenStream {
        let match_arms = self.expand_match_arms();

        quote! {
            (|
                mut __req: ::submillisecond::Request,
                mut __params: ::submillisecond::params::Params,
                mut __reader: ::submillisecond::core::UriReader
            | -> ::std::result::Result<::submillisecond::Response, ::submillisecond::RouteError> {
                if *__req.method() != ::submillisecond::http::Method::GET {
                    return ::std::result::Result::Err(::submillisecond::RouteError::RouteNotMatch(__req));
                }

                match __reader.read_to_end() {
                    #match_arms
                    _ => ::std::result::Result::Err(::submillisecond::RouteError::RouteNotMatch(__req)),
                }
            }) as ::submillisecond::Router
        }
    }

    fn expand_match_arms(&self) -> TokenStream {
        let arms = self.files.iter().map(|StaticFile { mime, path, content }| {
            let path = format!("/{path}");
            let mime = mime.to_string();
            let bytes = quote! { &[#( #content ),*] };

            quote! {
                #path => {
                    let mut headers = ::submillisecond::http::header::HeaderMap::new();
                    headers.insert(::submillisecond::http::header::CONTENT_TYPE, #mime.parse().unwrap());
                    ::std::result::Result::Ok(::submillisecond::response::IntoResponse::into_response((headers, #bytes as &'static [u8])))
                }
            }
        });

        quote! { #( #arms, )* }
    }
}

impl Parse for StaticDir {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let dir: LitStr = input.parse()?;
        let files = walk_dir(dir.value()).map_err(|err| syn::Error::new(dir.span(), err))?;

        Ok(StaticDir { files })
    }
}

#[derive(Debug)]
struct StaticFile {
    mime: Mime,
    path: String,
    content: Vec<u8>,
}

fn walk_dir<P>(base_path: P) -> io::Result<Vec<StaticFile>>
where
    P: AsRef<Path>,
{
    fn walk_nested(base_path: &Path, path: &Path) -> io::Result<Vec<StaticFile>> {
        let dir = fs::read_dir(path)?;
        let mut static_files = Vec::new();
        for entry in dir {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                println!("{:?}", entry.path());
                static_files.extend(walk_nested(base_path, &entry.path())?.into_iter());
            } else {
                let entry_path = entry.path();
                let entry_path = entry_path
                    .strip_prefix(&base_path)
                    .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
                let mime = mime_guess::from_path(&entry_path)
                    .first()
                    .unwrap_or(mime::TEXT_PLAIN);

                let content = fs::read(entry.path())?;

                static_files.push(StaticFile {
                    mime,
                    path: entry_path
                        .to_str()
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::Other,
                                "unable to convert path to UTF-8 string",
                            )
                        })?
                        .to_string(),
                    content,
                });
            }
        }

        Ok(static_files)
    }

    walk_nested(base_path.as_ref(), base_path.as_ref())
}
