mod controller;
use controller::data_base::make_db_pool;
use controller::{check_login, login, JwtClaims, RoomMessage, SECRET_KEY,connect};
use salvo::jwt_auth::{QueryFinder};
use salvo::{
    prelude::*,
    serve_static::StaticDir,
    session::{CookieStore, Session, SessionDepotExt, SessionHandler},
};
use tokio::sync::mpsc::{self, UnboundedSender};
use std::collections::{BTreeMap};
use futures_util::{FutureExt, StreamExt,stream::{SplitSink}, SinkExt,sink::{Send},Sink};
use salvo::{ws::{WebSocket,Message}};

struct RoomMessageSender(UnboundedSender<RoomMessage>);
#[async_trait]
impl Handler for RoomMessageSender{
	async fn handle(&self, _req: &mut Request, depot: &mut Depot, _res: &mut Response, ctrl: &mut FlowCtrl){
		depot.insert("roomSender", self.0.clone());
	} 
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = make_db_pool("mysql://root:970252187@localhost/wetalk").await?;
    let auth_handler: JwtAuth<JwtClaims> = JwtAuth::new(SECRET_KEY.to_owned())
        .with_finders(vec![
            Box::new(QueryFinder::new("token")),
            // Box::new(CookieFinder::new("jwt_token")),
        ])
        .with_response_error(false);

    let router = Router::with_path("<**path>").get(
        StaticDir::new(["static"])
            .with_defaults("index.html")
            .with_listing(true),
    );
	let mut room_map = BTreeMap::<String,BTreeMap<String,SplitSink<WebSocket, Message>>>::new();
    let (sender, mut reader) = mpsc::unbounded_channel::<RoomMessage>();
    let _room_task = tokio::spawn(async move {
        while let Some(msg) = reader.recv().await {
            match msg {
                RoomMessage::Open(ws, room_token, identity_token) => {
					match room_map.get_mut(&room_token){
						Some(v)=>{
							v.insert(identity_token, ws);
						}
						None=>{
							let mut value = BTreeMap::new();
							value.insert(identity_token, ws);
							room_map.insert(room_token, value);
						}
					}
				},
                RoomMessage::Receive(message, room_token, identity_token) => {
					match room_map.get_mut(&room_token){
						Some(v)=>{
							for (token,ws) in v{
								if token != &identity_token{
									let _ = ws.send(message.clone()).await;
								}
							}
							// if let Some(ws) = v.get_mut(&identity_token){
							// 	let _ = ws.send(message).await;
							// }
						}
						None=>{}
					}
				},
                RoomMessage::Close(room_token, identity_token) => {
					if room_map.get_mut(&room_token).is_some(){
						room_map.get_mut(&room_token).unwrap().remove(&identity_token);
					}
				},
            }
        }
    });
    //let api_router = Router::new().hoop(auth_handler).push(Router::with_path("chat").hoop(RoomMessageSender(sender)).handle(connect));
	let api_router = Router::new().push(router);
    let api_router = api_router.push(Router::with_path("login").post(login));
    let api_router = api_router.push(Router::with_path("check").post(check_login));

	let websocket_router = Router::with_path("chat").hoop(RoomMessageSender(sender)).handle(connect);

	let total_router = Router::new().hoop(auth_handler);
	
	let total_router = total_router.push(websocket_router);
	let total_router = total_router.push(api_router);
	
	// let total_router = Router::new().hoop(auth_handler).push(Router::with_path("chat").hoop(RoomMessageSender(sender)).handle(connect));
	// let total_router = total_router.push(api_router);
	let acceptor = TcpListener::bind("0.0.0.0:8080");
	Server::new(acceptor).serve(total_router).await;
    Ok(())
}
