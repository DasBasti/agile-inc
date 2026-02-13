use redis::Commands;

pub fn connect(url:&str)->redis::Connection{
    let client = redis::Client::open(url).expect("redis open");
    client.get_connection().expect("redis conn")
}

pub fn ping(conn:&mut redis::Connection)->bool{
    let r: String = redis::cmd("PING").query(conn).unwrap_or_default();
    r=="PONG"
}
