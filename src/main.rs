mod controller;
use controller::data_base::make_db_pool;
use controller::{check_login, login, JwtClaims, RoomMessage,connect,history,upload};
use salvo::jwt_auth::{QueryFinder, HeaderFinder};
use salvo::{
    prelude::*,
    serve_static::StaticDir,
};
use tokio::sync::mpsc::{self, UnboundedSender};
use std::collections::{BTreeMap};
use futures_util::{stream::{SplitSink}, SinkExt,};
use salvo::{ws::{WebSocket,Message}};

struct RoomMessageSender(UnboundedSender<RoomMessage>);
#[async_trait]
impl Handler for RoomMessageSender{
	async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl){
		depot.insert("roomSender", self.0.clone());
		ctrl.call_next(req, depot, res).await;
	} 
}

struct SecretKeyWrapper(String);
#[async_trait]
impl Handler for SecretKeyWrapper{
	async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl){
		depot.insert("secret_key", self.0.clone());
		ctrl.call_next(req, depot, res).await;
	} 
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	tokio::fs::create_dir_all("./static/upload").await?;
	let config_file = tokio::fs::read_to_string("./config.json").await?;
	let config_json:serde_json::Value = serde_json::from_str(&config_file)?;
	let db_protocol = config_json.get("db_protocol").unwrap().as_str().unwrap();
	let secret_key = config_json.get("secret_key").unwrap().as_str().unwrap();
    let _ = make_db_pool(db_protocol).await?;
    let auth_handler: JwtAuth<JwtClaims> = JwtAuth::new(secret_key.to_owned())
        .with_finders(vec![
            Box::new(QueryFinder::new("token")),
			Box::new(HeaderFinder::new()),
            // Box::new(CookieFinder::new("jwt_token")),
        ])
        .with_response_error(false);


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

    let api_router = Router::new().push(Router::with_path("login").post(login));
    let api_router = api_router.push(Router::with_path("check").post(check_login));
	let api_router = api_router.push(Router::with_path("history").get(history));

	let websocket_router = Router::with_path("chat").hoop(RoomMessageSender(sender)).handle(connect);

	let total_router = Router::new().hoop(auth_handler).hoop(SecretKeyWrapper(secret_key.to_owned()));
	let total_router = total_router.push(api_router);
	let total_router = total_router.push(websocket_router);
	let total_router = total_router.push(Router::with_path("upload").post(upload));

	let static_resource = Router::with_path("static/<**>").get(
        StaticDir::new(["static"])
            .with_defaults("index.html")
            .with_listing(true),
    );
	let total_router = total_router.push(static_resource);
	let www_resource = Router::with_path("<**>").get(
        StaticDir::new(["www"])
            .with_defaults("index.html")
            .with_listing(true),
    );
	let total_router = total_router.push(www_resource);

	
	let acceptor = TcpListener::bind("0.0.0.0:8080");
	Server::new(acceptor).serve(total_router).await;
    Ok(())
}
