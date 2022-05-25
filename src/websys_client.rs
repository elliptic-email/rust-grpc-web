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
    support_streaming: bool,
) -> TokenStream {
    let client_mod = quote::format_ident!("{}_client", naive_snake_case(service.name()));
    let service_name = quote::format_ident!("{}", service.name());
    let methods = generate_methods(service, proto_path, compile_well_known_types);
    let streaming_support = generate_streaming_support(support_streaming);

    let status = generate_grpc_status();

    quote! {
        /// Generated client implementations.
        pub mod #client_mod {
            #![allow(unused_variables, dead_code, missing_docs)]
            use prost::Message;
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

                fn frame_request<T: prost::Message>(request: T) -> Vec<u8> {
                    let mut proto_buffer: Vec<u8> = Vec::new();
                    request.encode(&mut proto_buffer).unwrap();
                    let mut frame: Vec<u8> = vec!(0_u8);
                    frame.append(&mut (proto_buffer.len() as u32).to_be_bytes().to_vec());
                    frame.append(&mut proto_buffer);

                    frame
                }

                #streaming_support
            }

            #status
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

fn generate_streaming_support(
    streaming_support: bool,
) -> TokenStream {

    if streaming_support {
        return quote! {

            pub fn initialise_stream<T: prost::Message>(request: T, web_socket: &web_sys::WebSocket) {
                let headers = "content-type: application/grpc-web+proto\r\nx-grpc-web: 1\r\n";
                web_socket.send_with_u8_array(headers.as_bytes()).unwrap();
            
                // Send frame
                let frame = Self::websocket_frame_request(request);
                web_socket.send_with_u8_array(&frame).unwrap();
            
                // Send finish
                let bytes: Vec<u8> = vec!(1);
                web_socket.send_with_u8_array(&bytes).unwrap();
            }

            fn websocket_host(&self) -> String {
                let ssl_replace = str::replace(&self.host, "https", "wss");
                str::replace(&ssl_replace, "http", "ws")
            }

            // Websockets take an extra byte, not sure why.
            // https://github.com/improbable-eng/grpc-web/blob/84ab65f9526bd73430fb786dced98135186dd099/client/grpc-web/src/transports/websocket/websocket.ts#L30
            pub fn websocket_frame_request<T: prost::Message>(request: T) -> Vec<u8> {
                let mut proto_buffer: Vec<u8> = Vec::new();
                request.encode(&mut proto_buffer).unwrap();
                let mut frame: Vec<u8> = vec!(0,0);
                frame.append(&mut (proto_buffer.len() as u32).to_be_bytes().to_vec());
                frame.append(&mut proto_buffer);

                frame
            }
        }
    } else {
        return quote! {
        }
    }
}

fn generate_server_streaming<T: Method, S: Service>(
    _service: &S,
    _method: &T,
    _proto_path: &str,
    _compile_well_known_types: bool,
) -> TokenStream {

    quote! {
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

            let frame = Self::frame_request(request);

            let client = reqwest::Client::new();
            let result = client.post(format!("{}{}", &self.host, #url))
                .header(reqwest::header::CONTENT_TYPE, "application/grpc-web+proto")
                .header("x-grpc-web", "1")
                .body(frame)
                .send()
                .await?;

            let headers = result.headers();
            if headers.contains_key("grpc-status") && headers.contains_key("grpc-message") {
                let grpc_status = headers["grpc-status"].to_str()?.to_string().parse::<u32>()?;
                let grpc_message = headers["grpc-message"].to_str()?.to_string();

                let status: GrpcStatus = GrpcStatus::from_code(grpc_status, grpc_message);

                if status.is_error(){
                    return Err(Box::new(status));
                }
            }

            let mut bytes = result.bytes()
                .await?;

            // Todo read in the whole length of the buffer.
            let mut dst = [0u8; 4];
            dst.clone_from_slice(&bytes[1..5]);
            let size = u32::from_be_bytes(dst);
            let frame = bytes.split_to(5 + (size as usize));

            let s = #response::decode(frame.slice(5..))?;
            Ok(s)
        }
    }
}

fn generate_grpc_status() -> TokenStream {
    quote! {
        #[derive(Debug)]
        pub enum GrpcStatus{
            Ok(String),
            Cancelled(String),
            Unknown(String),
            InvalidArgument(String),
            DeadlineExceeded(String),
            NotFound(String),
            AlreadyExists(String),
            PermissionDenied(String),
            ResourceExhausted(String),
            FailedPrecondition(String),
            Aborted(String),
            OutOfRange(String),
            Unimplemented(String),
            Internal(String),
            Unavailable(String),
            DataLoss(String),
            Unauthenticated(String),
            Other(String)
        }
        
        impl GrpcStatus{
            pub fn from_code(code: u32, message: String) -> Self {
                match code {
                    0 => Self::Ok(message),
                    1 => Self::Cancelled(message),
                    2 => Self::Unknown(message),
                    3 => Self::InvalidArgument(message),
                    4 => Self::DeadlineExceeded(message),
                    5 => Self::NotFound(message),
                    6 => Self::AlreadyExists(message),
                    7 => Self::PermissionDenied(message),
                    8 => Self::ResourceExhausted(message),
                    9 => Self::FailedPrecondition(message),
                    10 => Self::Aborted(message),
                    11 => Self::OutOfRange(message),
                    12 => Self::Unimplemented(message),
                    13 => Self::Internal(message),
                    14 => Self::Unavailable(message),
                    15 => Self::DataLoss(message),
                    16 => Self::Unauthenticated(message),
                    _ => Self::Other(message)
                }
            }
        
            pub fn is_error(&self) -> bool{
                match self {
                    GrpcStatus::Ok(_) => false,
                    _ => true
                }
            }
        }
        
        impl std::fmt::Display for GrpcStatus {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "GrpcStatus::{:?}", self)
            }
        }
        
        impl std::error::Error for GrpcStatus {}
    }
}