pub mod data_base;
use futures_util::{FutureExt, StreamExt,stream::{SplitSink}, SinkExt,sink::{Send},Sink};
use chrono::Local;
use data_base::{
    db_pool,
    model::{participant_table, prelude::*, room_table},
};
use salvo::{
    prelude::*,
    session::{Session, SessionDepotExt},
};
use sea_orm::{
    ActiveModelBehavior, ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, PaginatorTrait,
    QueryFilter,
};
use serde_json::json;

use salvo::{ws::{WebSocket,Message}};

use serde::{Serialize,Deserialize};

use time::{Duration, OffsetDateTime};
use tokio::sync::mpsc::{self, UnboundedSender};

pub const SECRET_KEY: &str = "123456789123456789";


#[derive(Debug, Serialize, Deserialize)]
pub struct JwtClaims {
    name: String,
	identity_token:String,
	room_token:String,
	exp:i64
}



#[macro_export]
macro_rules! try_query_form {
    ($req:expr, $key:expr) => {
        match $req.form($key).await {
            Some(v) => v,
            None => return Err(anyhow::format_err!("{} is required", $key).into()),
        }
    };
}
struct HandleError<const IS_JSON:bool = true>(String);
impl<T:ToString,const IS_JSON:bool> From<T> for HandleError<IS_JSON> where T:Into<anyhow::Error>{
    fn from(value: T) -> Self {
        HandleError(value.to_string())
    }
}

#[async_trait]
impl<const IS_JSON:bool> Writer for HandleError<IS_JSON> {
    async fn write(mut self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
		if IS_JSON{
			let r = json!({
				"success":false,
				"reason":{
					"msg":self.0,
					"code":400
				}
			});
			res.render(Text::Json(r.to_string()));
			return;
		}
    }
}

#[handler]
pub async fn login(req: &mut Request, res: &mut Response, depot: &mut Depot) -> Result<(),HandleError> {
    let name: String = try_query_form!(req, "name");
    let pass: String = try_query_form!(req, "pass");
    let room_token: String = try_query_form!(req, "room_token");
    let db = db_pool();
    let room_exist = RoomTable::find()
        .filter(room_table::Column::RoomToken.eq(&room_token))
        .count(&*db)
        .await?;
    if room_exist == 0 {
        let r = serde_json::json!({
             "reason":{
                 "msg":"不存在该房间",
                 "code":400
             },
             "success":false
        });
        res.render(Text::Json(r.to_string()));
        return Ok(());
    }
    let pass_md5 = format!("{:x}", md5::compute(pass));
    let result = ParticipantTable::find()
        .filter(participant_table::Column::Name.eq(&name))
        .filter(participant_table::Column::RoomToken.eq(&room_token))
        .filter(participant_table::Column::Pass.eq(&pass_md5))
        .count(&*db)
        .await?;
	let identity_token = md5::compute(format!("{name}/{pass_md5}/{room_token}"));
	let identity_token = format!("{identity_token:x}");
	let exp = OffsetDateTime::now_utc() + Duration::days(1);
	let jwt_data = JwtClaims{
		name:name.clone(),
		identity_token:identity_token,
		exp:exp.unix_timestamp(),
		room_token:room_token.clone()
		// exp:exp.unix_timestamp(),
	};
	let token = jsonwebtoken::encode(
		&jsonwebtoken::Header::default(),
		&jwt_data,
		&jsonwebtoken::EncodingKey::from_secret(SECRET_KEY.as_bytes()),
	)?;
	println!("{token}");
    if result == 0 {
        let mut info = participant_table::ActiveModel::new();
        info.name = ActiveValue::Set(name);
        info.room_token = ActiveValue::Set(room_token);
		info.pass = ActiveValue::Set(pass_md5);
        let now = Local::now();
        info.created_time = ActiveValue::Set(Some(now.clone().naive_local()));
        info.updated_time = ActiveValue::Set(Some(now.naive_local()));
        info.insert(&*db).await?;
        // let mut session = Session::new();
        // session.insert("identity_token", identity_token)?;
        // depot.set_session(session);
        let r = serde_json::json!({
             "token": token,
             "success":true
        });
        res.render(Text::Json(r.to_string()));
    } else {
        let r = serde_json::json!({
			"token": token,
			"success":true
        });
        res.render(Text::Json(r.to_string()));
    }
    Ok(())
}

#[handler]
pub async fn check_login(req: &mut Request, res: &mut Response, depot: &mut Depot) -> Result<(),HandleError> {
	// let r = depot.jwt_auth_state();
	// res.render(Text::Plain(format!("{r:?}")));
	if let JwtAuthState::Authorized =  depot.jwt_auth_state(){
		let data = depot.jwt_auth_data::<JwtClaims>().unwrap();
		let r = &data.claims;
		res.render(Text::Plain(format!("{r:?}")));
	}else{
		res.render(Text::Plain("unauthorized"));
	}
	Ok(())
}
#[derive(Debug)]
pub enum RoomMessage{
	Open(SplitSink<WebSocket, Message>,String,String),
	Receive(Message,String,String),
	Close(String, String)
}


#[handler]
pub async fn connect(req: &mut Request, res: &mut Response,depot: &mut Depot) -> Result<(), StatusError> {
	println!("connect, connect connectconnectconnectconnect");
	let (name,identity_token, room_token) = {
		match depot.jwt_auth_state(){
			JwtAuthState::Authorized => {
				match depot.jwt_auth_data::<JwtClaims>(){
					Some(d)=>{
						(d.claims.name.clone(),d.claims.identity_token.clone(),d.claims.room_token.clone())
					}
					None=>{
						return Err(StatusError::bad_request());
					}
				}
			},
			_=>{
				println!("bad_request");
				return Err(StatusError::bad_request());
			}
		}
	};
	println!("prepare to connect");
	let room_sender = if let Some(v) = depot.get::<UnboundedSender<RoomMessage>>("roomSender"){v}else{
		return Err(StatusError::bad_request());
	};
	let room_sender = room_sender.clone();
	WebSocketUpgrade::new()
	.upgrade(req, res, | ws| async move {
		let (ws_sender, mut ws_reader) = ws.split();
		//ws_sender.start_send();
		//<SplitSink<WebSocket, Message> as Sink<Message>>::start_send(& mut ws_sender, item);
		//ws_sender.send(item).await;
		room_sender.send(RoomMessage::Open(ws_sender, room_token.clone(), identity_token.clone())).unwrap();
		let message = Message::text(format!("{name} 加入了聊天室"));
		room_sender.send(RoomMessage::Receive(message, room_token.clone(), identity_token.clone())).unwrap();
		while let Some(msg) = ws_reader.next().await{
			//println!("receive message");
			let Ok(msg) = msg else{
				// client disconnected
				let _ = room_sender.send(RoomMessage::Close(room_token.clone(), identity_token.clone())).unwrap();
				let message = Message::text(format!("{name} 退出了聊天室"));
				room_sender.send(RoomMessage::Receive(message, room_token.clone(), identity_token.clone())).unwrap();
				return;
			};
			room_sender.send(RoomMessage::Receive(msg,room_token.clone(), identity_token.clone())).unwrap();
		}
		room_sender.send(RoomMessage::Close(room_token.clone(), identity_token.clone())).unwrap();
		let message = Message::text(format!("{name} 退出了聊天室"));
		room_sender.send(RoomMessage::Receive(message, room_token.clone(), identity_token.clone())).unwrap();
		// while let Some(msg) = ws.recv().await {
		// 	let msg = if let Ok(msg) = msg {
		// 		msg
		// 	} else {
		// 		// client disconnected
		// 		return;
		// 	};

		// 	if ws.send(msg).await.is_err() {
		// 		// client disconnected
		// 		return;
		// 	}
		// }
	})
	.await
}
