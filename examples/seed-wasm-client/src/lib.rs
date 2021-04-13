// (Lines like the one below ignore selected Clippy rules
//  - it's useful when you want to check your code with `cargo make verify`
// but some rules are too "annoying" or are not applicable for your case.)
#![allow(clippy::wildcard_imports)]

pub mod hello_world {
    include!(concat!(env!("OUT_DIR"), concat!("/helloworld.rs")));
}

use seed::{prelude::*, *};
use hello_world::{HelloRequest, HelloReply, greeter_client};

// ------ ------
//     Init
// ------ ------

// `init` describes what should happen when your app started.
fn init(_: Url, _: &mut impl Orders<Msg>) -> Model {
    Model::default()
}

// ------ ------
//     Model
// ------ ------

// `Model` describes our app state.
type Model = i32;

// ------ ------
//    Update
// ------ ------

// (Remove the line below once any of your `Msg` variants doesn't implement `Copy`.)
//#[derive(Copy, Clone)]
// `Msg` describes the different events you can modify state with.
enum Msg {
    Increment,
    Fetched(Result<HelloReply, Box<dyn std::error::Error>>)
}

// `update` describes how to handle each `Msg`.
fn update(msg: Msg, model: &mut Model, orders: &mut impl Orders<Msg>) {
    match msg {
        Msg::Increment => {
            orders.skip().perform_cmd({
                async { Msg::Fetched(send_message("World2!").await) }
            });
        },

        Msg::Fetched(Ok(response_data)) => {
            log(response_data.message);
        },

        Msg::Fetched(Err(response_data)) => {
            log("Not a result");
        }
    }
}

async fn send_message(name: &str) -> Result<HelloReply, Box<dyn std::error::Error>> {
    
    let client = greeter_client::Greeter::new(String::from("http://localhost:8080"));
            
    let req = HelloRequest {
        name: String::from(name)
    };
    client.say_hello(req).await
}

// ------ ------
//     View
// ------ ------

// (Remove the line below once your `Model` become more complex.)
#[allow(clippy::trivially_copy_pass_by_ref)]
// `view` describes what to display.
fn view(model: &Model) -> Node<Msg> {
    div![
        "This is a counter: ",
        C!["counter"],
        button![model, ev(Ev::Click, |_| Msg::Increment),],
    ]
}

// ------ ------
//     Start
// ------ ------

// (This function is invoked by `init` function in `index.html`.)
#[wasm_bindgen(start)]
pub fn start() {
    // Mount the `app` to the element with the `id` "app".
    App::start("app", init, update, view);
}
