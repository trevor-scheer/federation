use tide::{Request, Response, StatusCode};

use stargate::{Stargate};
use stargate::transports::http::*;

#[derive(Clone)]
struct State<'app> {
    stargate: Stargate<'app>
}

#[async_std::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tide::log::start();
    /*

        The main fn reads from the input arguments to running the binary
        then it starts up a server and begins to listen on the correct port

    */
    // XXX read from arguments
    let schema = include_str!("./acephei.graphql");
    let route = String::from("/");
    let address = String::from("127.0.0.1:8080");

    let stargate = Stargate::new(schema);

    let mut server = tide::with_state(State { stargate });
    // allow studio
    server.with(get_studio_middleware());


    server.at(route.as_str())
        .post(|mut req: Request<State<'static>>| async move {

            let request_context = req.build_request_context().await?;

            let state = req.state();

            let resp = state.stargate.execute_query(&request_context).await;

            Response::new(StatusCode::Ok).format_graphql_response(resp)
        });

    server.listen(address.as_str()).await?;
    Ok(())
}