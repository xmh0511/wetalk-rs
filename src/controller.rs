pub mod data_base;
use futures_util::{ StreamExt,stream::{SplitSink},};
use chrono::Local;
use data_base::{
    db_pool,
    model::{participant_table, prelude::*, room_table,message_table},
};
use salvo::{
    prelude::*,
};
use sea_orm::{
    ActiveModelBehavior, ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder,
};
use serde_json::json;

use salvo::{ws::{WebSocket,Message}};

use serde::{Serialize,Deserialize};

use time::{Duration, OffsetDateTime};
use tokio::sync::mpsc::{ UnboundedSender};




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
pub struct HandleError<const IS_JSON:bool = true>(String, u16);
impl<T:ToString,const IS_JSON:bool> From<T> for HandleError<IS_JSON> where T:Into<anyhow::Error>{
    fn from(value: T) -> Self {
        HandleError(value.to_string(),400)
    }
}

impl<const IS_JSON:bool> HandleError<IS_JSON>{
	pub fn new<T:ToString>(v:T,code:u16)->Self{
		HandleError(v.to_string(),code)
	}
}

#[macro_export]
macro_rules! uniform_error {
	($e:expr) => {
		HandleError::new($e,400)
	};
	($e:expr, $code:expr)=>{
		HandleError::new($e,$code)
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
					"code":self.1
				}
			});
			res.render(Text::Json(r.to_string()));
			return;
		}else{
			res.render(Text::Plain("request fails"));
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
	let Some(secret_key) = depot.get::<String>("secret_key") else{
		return Err(anyhow::format_err!("cannot generate token").into());
	};
	let token = jsonwebtoken::encode(
		&jsonwebtoken::Header::default(),
		&jwt_data,
		&jsonwebtoken::EncodingKey::from_secret(secret_key.as_bytes()),
	)?;
	//println!("{token}");
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
pub async fn check_login(_req: &mut Request, res: &mut Response, depot: &mut Depot) -> Result<(),HandleError> {
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
fn get_person_from_jwt(depot: &mut Depot)->Result<(String,String,String),HandleError>{
	let (name,identity_token, room_token) = {
		match depot.jwt_auth_state(){
			JwtAuthState::Authorized => {
				match depot.jwt_auth_data::<JwtClaims>(){
					Some(d)=>{
						(d.claims.name.clone(),d.claims.identity_token.clone(),d.claims.room_token.clone())
					}
					None=>{
						//return Err(anyhow::format_err!("authorization is not passed").into());
						return Err(uniform_error!(anyhow::format_err!("authorization is not passed"),404));
					}
				}
			},
			_=>{
				return Err(uniform_error!(anyhow::format_err!("authorization is not passed"),404));
			}
		}
	};
	Ok((name,identity_token,room_token))
}
#[handler]
pub async fn history(_req: &mut Request, res: &mut Response, depot: &mut Depot) -> Result<(),HandleError> {
	let (name,identity_token, room_token) = get_person_from_jwt(depot)?;
	let db = db_pool();
	let info = MessageTable::find().filter(message_table::Column::RoomToken.eq(&room_token)).order_by_asc(message_table::Column::CreatedTime).into_json().all(&*db).await?;
	let room_info = RoomTable::find().filter(room_table::Column::RoomToken.eq(&room_token)).one(&*db).await?;
	let Some(room_info) = room_info else{
		return Err(anyhow::format_err!("{room_token}的房间不存在").into());
	};
	let json = json!({
		"msg":{
			"list":info,
			"me":{
				"token":identity_token,
				"name":name,
			},
			"room_token":room_token,
			"room_name":room_info.room_name
		},
		"success":true
	});
	res.render(Text::Json(json.to_string()));
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
	let Ok((name,identity_token, room_token)) = get_person_from_jwt(depot) else{
		return Err(StatusError::bad_request());
	};
	//println!("prepare to connect");
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
		let add_prompt = json!({
			"type":1,
			"message":format!("{name} 加入了聊天室")
		});
		let message = Message::text(add_prompt.to_string());
		room_sender.send(RoomMessage::Receive(message, room_token.clone(), identity_token.clone())).unwrap();
		let exit_prompt = json!({
			"type":3,
			"message":format!("{name} 退出了聊天室")
		});
		while let Some(msg) = ws_reader.next().await{
			//println!("receive message");
			let Ok(msg) = msg else{
				// client disconnected
				let _ = room_sender.send(RoomMessage::Close(room_token.clone(), identity_token.clone())).unwrap();
				let message = Message::text(exit_prompt.to_string());
				room_sender.send(RoomMessage::Receive(message, room_token.clone(), identity_token.clone())).unwrap();
				return;
			};
			if let Ok(s) = msg.to_str(){
				match serde_json::from_str::<serde_json::Value>(s){
					Ok(v)=>{
						match v.get("type"){
							Some(v)=>{
								//println!("{v:?}");
								if let Some(4) = v.as_u64(){
									//println!("control message comming");
									continue;
								}else if let None = v.as_u64(){
									continue;
								}
							}
							None=>{ continue;}
						}
					}
					Err(_)=>{
						continue;
					}
				};
				let db = db_pool();
				let mut info = message_table::ActiveModel::new();
				info.identity_token = ActiveValue::set(identity_token.clone());
				info.owner_name = ActiveValue::set(name.clone());
				info.room_token = ActiveValue::set(room_token.clone());
				let now = Local::now();
				info.created_time  = ActiveValue::set(Some(now.naive_local()));
				info.updated_time  = ActiveValue::set(Some(now.naive_local()));
				info.message = ActiveValue::set(s.to_owned());
				let _r = info.insert(&*db).await;
				//println!("insert to db: {r:?}");
				room_sender.send(RoomMessage::Receive(msg,room_token.clone(), identity_token.clone())).unwrap();
			}
		}
		room_sender.send(RoomMessage::Close(room_token.clone(), identity_token.clone())).unwrap();
		let message = Message::text(exit_prompt.to_string());
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

#[handler]
pub async fn upload(req: &mut Request, res: &mut Response,depot: &mut Depot)->Result<(),HandleError> {
	let (_name,_identity_token, _room_token) = get_person_from_jwt(depot)?;
    let file = req.file("file").await;
    if let Some(file) = file {
		let Some(original_name) = file.name() else{
			return Err(anyhow::format_err!("file'name not found in request").into());
		};
		let is_picture = {
			match imghdr::from_file(file.path()){
				Ok(Some(_)) => true,
				_=> false
			}
		};
		//let some_size = file.size();
		let path_size = if let Ok(x)  = file.path().metadata(){
			x.len()
		}else{
			0
		};
		// println!("size from path {}",path_size);
		// println!("some_size {:?}",some_size);
		//let file_size = file.size().unwrap_or(0);
		let extension = file.path().extension().unwrap_or_default().to_str().unwrap_or_default().to_string();
		let name = uuid::Uuid::new_v4();
        let dest = format!("./static/upload/{name}.{extension}");
        //println!("{}", dest);
		//let path = file.path().to_str().unwrap();
		//println!("uploaded file path: {path}");
        if let Err(e) = std::fs::copy(&file.path(), std::path::Path::new(&dest)) {
			return Err(anyhow::format_err!("file not found in request: {}",e).into());
        };
		let json = json!({
			"msg":{
				"file_name":original_name,
				"url": format!("/static/upload/{name}.{extension}"),
				"is_picture":is_picture,
				"size":path_size
			},
			"success":true
		});
        res.render(Text::Json(json.to_string()));
    } else {
		return Err(anyhow::format_err!("file not found in request").into());
    };
	Ok(())
}

