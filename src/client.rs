use super::{Method, Service};
use crate::{generate_doc_comments, naive_snake_case};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

/// Generate service for client.
///
/// This takes some `Service` and will generate a `TokenStream` that contains
/// a public module with the generated client.
pub fn generate<T: Service>(
    service: &T,
    proto_path: &str,
    compile_well_known_types: bool,
) -> TokenStream {
    let client_mod = quote::format_ident!("{}_client", naive_snake_case(&service.name()));
    let service_name = quote::format_ident!("{}", service.name());
    let methods = generate_methods(service, proto_path, compile_well_known_types);

    quote! {
        /// Generated client implementations.
        pub mod #client_mod {
            #![allow(unused_variables, dead_code, missing_docs)]
            use prost::Message;
            use tokio_tungstenite::{connect_async, tungstenite};
            use http::Request;
            use futures_util::{SinkExt, StreamExt};
            pub struct #service_name {
                host: String
            }

            impl #service_name {
                #methods

                pub fn new(host: String) -> #service_name {
                    #service_name {
                        host
                    }
                }
            }
        }
    }
}

fn generate_methods<T: Service>(
    service: &T,
    proto_path: &str,
    compile_well_known_types: bool,
) -> TokenStream {
    let mut stream = TokenStream::new();

    for method in service.methods() {

        stream.extend(generate_doc_comments(method.comment()));

        let method = match (method.client_streaming(), method.server_streaming()) {
            (false, false) => generate_unary(service, method, proto_path, compile_well_known_types),
            (false, true) => generate_server_streaming(service, method, proto_path, compile_well_known_types),
            (true, false) => {
                TokenStream::new()
            }
            (true, true) => TokenStream::new(),
        };

        stream.extend(method);
    }

    stream
}

fn generate_server_streaming<T: Method, S: Service>(
    service: &S,
    method: &T,
    proto_path: &str,
    compile_well_known_types: bool,
) -> TokenStream {
    let ident = format_ident!("{}", method.name());
    let (request, response) = method.request_response_name(proto_path, compile_well_known_types);
    let url = format!("/{}.{}/{}", service.package(), service.name(), method.identifier());

    quote! {
        pub async fn #ident(
            &self,
            request: #request
        ) -> Result<(), Box<dyn std::error::Error>> {
            let mut proto_buffer: Vec<u8> = Vec::new();
            request.encode(&mut proto_buffer).unwrap();

            dbg!(&proto_buffer);
    
            let headers = "content-type: application/grpc-web+proto\r\nx-grpc-web: 1\r\n";
            let initial_msg = tungstenite::protocol::Message::binary(headers.as_bytes());

            let bytes: Vec<u8> = vec!(0,0,0,0,0,0);
            let bytes_msg = tungstenite::protocol::Message::binary(bytes);
        
            let bytes: Vec<u8> = vec!(1);
            let one_msg = tungstenite::protocol::Message::binary(bytes);

            let host = "ws://localhost:8080";
            
            let req: Request<()> = Request::builder()
                .uri(format!("{}{}", host, #url))
                .header("Sec-Websocket-Protocol", "grpc-websockets")
                .body(())
                .unwrap();

            let (mut ws_stream, _) = connect_async(req).await?;
            ws_stream.send(initial_msg).await?;
            ws_stream.send(bytes_msg).await?;
            ws_stream.send(one_msg).await?;
            while let Some(msg) = ws_stream.next().await {
                let msg = msg?;
                if msg.is_text() || msg.is_binary() {
                    dbg!(msg);
                }
            }

            Ok(())
        }
    }
}

fn generate_unary<T: Method, S: Service>(
    service: &S,
    method: &T,
    proto_path: &str,
    compile_well_known_types: bool,
) -> TokenStream {
    let ident = format_ident!("{}", method.name());
    let (request, response) = method.request_response_name(proto_path, compile_well_known_types);
    let url = format!("/{}.{}/{}", service.package(), service.name(), method.identifier());

    quote! {
        pub async fn #ident(
            &self,
            request: #request
        ) -> Result<#response, Box<dyn std::error::Error>> {
            let mut proto_buffer: Vec<u8> = Vec::new();
            request.encode(&mut proto_buffer).unwrap();
            let mut frame: Vec<u8> = Vec::new();
            frame.push(0 as u8);
            frame.append(&mut (proto_buffer.len() as u32).to_be_bytes().to_vec());
            frame.append(&mut proto_buffer);

            let client = reqwest::Client::new();
            let mut bytes = client.post(format!("{}{}", &self.host, #url))
                .header(reqwest::header::CONTENT_TYPE, "application/grpc-web+proto")
                .body(frame)
                .send()
                .await?
                .bytes()
                .await?;

            // Todo read in the whole length of the buffer.
            let count: &u8 = bytes.get(4).unwrap();
            let size = *count as usize;
            let frame = bytes.split_to(5 + size);

            let s = #response::decode(frame.slice(5..))?;
            Ok(s)
        }
    }
}