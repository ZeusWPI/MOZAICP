extern crate futures;
extern crate mozaic;
extern crate mozaic_derive;
extern crate serde;
extern crate serde_json;
extern crate tokio;

extern crate tracing;
extern crate tracing_subscriber;

use tracing_subscriber::{EnvFilter, FmtSubscriber};

use std::time;

use mozaic::modules::types::*;
use mozaic::modules::GameController;

use futures::executor::ThreadPool;

static WIDTH: usize = 8;
static HEIGHT: usize = 7;

struct Server {
    next_player: u64,
    field: Vec<Vec<u64>>,
}
impl GameController for Server {
    fn step(&mut self, turns: Vec<PlayerMsg>) -> Vec<HostMsg> {
        let mut updates = Vec::new();
        if let Some(PlayerMsg {
            id,
            data: Some(data),
        }) = turns.get(0)
        {
            if self.next_player == *id {
                let column: u32 = match data.value.parse() {
                    Ok(num) => num,
                    Err(_) => return vec![HostMsg::Kick(*id)],
                };

                if self.field[column as usize].len() < HEIGHT {
                    self.field[column as usize].push(*id);
                    if check_if_won(&self.field) {
                        updates.push(HostMsg::new(
                            String::from("You Win"),
                            Some(self.next_player),
                        ));
                        updates.push(HostMsg::new(
                            String::from("You Lose"),
                            Some(1 - self.next_player),
                        ));
                        updates.push(HostMsg::Kick(0));
                        updates.push(HostMsg::Kick(1));
                    } else {
                        updates.push(HostMsg::new(String::from("Ok"), None));
                    }

                    self.next_player = (self.next_player + 1) % 2
                }
            }
        }

        updates
    }
}

fn check_if_won(field: &Vec<Vec<u64>>) -> bool {
    has_hor(field) || has_vert(field) || has_diag_bl_tr(&field) || has_diag_tl_br(&field)
}

fn has_diag_bl_tr(field: &Vec<Vec<u64>>) -> bool {
    for i in 0..WIDTH - 3 {
        let field_diag_bl_tr: Vec<Vec<u64>> = field
            .iter()
            .skip(i)
            .zip(0..WIDTH)
            .map(|(col, shift)| col.iter().skip(shift).cloned().collect())
            .collect();
        // println!("{:?}", field_diag_bl_tr);

        if has_hor(&field_diag_bl_tr) {
            return true;
        }
    }

    false
}

fn has_diag_tl_br(field: &Vec<Vec<u64>>) -> bool {
    let mut field = field.clone();
    field.reverse();
    return has_diag_bl_tr(&field);
}

fn has_hor(field: &Vec<Vec<u64>>) -> bool {
    let max_length = field.iter().map(|x| x.len()).max().expect("No max?");
    // horizontal
    for linei in 0..max_length {
        let mut history = 0;
        let mut last = 999;
        for column in field {
            if linei < column.len() {
                let value = column[linei];
                if last != value {
                    history = 0;
                    last = value;
                } else {
                    history += 1;
                }
                if history == 3 {
                    return true;
                }
            } else {
                history = 0;
                last = 999;
            }
        }
    }

    false
}

fn has_vert(field: &Vec<Vec<u64>>) -> bool {
    for column in field {
        let mut history = 0;
        let mut last = 999;
        for &value in column.iter() {
            if last != value {
                history = 1;
                last = value;
            } else {
                history += 1;
            }

            if history == 4 {
                return true;
            }
        }
    }

    false
}

impl Server {
    fn new() -> Server {
        let next_player = if rand::random() { 0 } else { 1 };

        let field = (0..WIDTH).map(|_| Vec::new()).collect();

        Server { next_player, field }
    }
}

use mozaic::modules::GameBuilder;
#[tokio::main]
async fn main() {
    let sub = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .finish();
    tracing::subscriber::set_global_default(sub).unwrap();
    {
        let pool = ThreadPool::builder().create().unwrap();

        let players = vec![0, 1];
        let game = Server::new();
        let builder = GameBuilder::new(players.clone(), game);

        builder.run(pool).await;
    }

    std::thread::sleep(time::Duration::from_millis(100));
}

#[test]
fn check_hor() {
    let f1 = vec![
        vec![1, 0, 0, 1, 0],
        vec![0, 0, 0, 1],
        vec![0, 0],
        vec![0, 1],
        vec![0],
        vec![1],
        vec![],
        vec![],
    ];
    assert!(has_hor(&f1));

    let f2 = vec![
        vec![1, 0, 0, 1, 0],
        vec![0, 0, 0, 1],
        vec![0, 0],
        vec![0, 1],
        vec![1],
        vec![0],
        vec![0],
        vec![0],
    ];
    assert!(!has_hor(&f2));
}

#[test]
fn check_vert() {
    let f1 = vec![
        vec![1, 0, 0, 1, 0],
        vec![1, 1, 1, 1],
        vec![0, 0],
        vec![0, 1],
        vec![0],
        vec![0],
        vec![],
        vec![],
    ];
    assert!(has_vert(&f1));

    let f2 = vec![
        vec![1, 0, 0, 1, 0],
        vec![0, 0, 0, 1],
        vec![0, 0],
        vec![1, 1],
        vec![0],
        vec![1],
        vec![],
        vec![],
    ];
    assert!(!has_vert(&f2));
}

#[test]
fn check_diag_bl_tr() {
    let f1 = vec![
        vec![1, 0, 0, 1, 0],
        vec![1, 1, 1, 1],
        vec![0, 0, 1],
        vec![0, 1, 0, 0],
        vec![0, 0, 1],
        vec![0, 1, 0, 1],
        vec![0, 1, 0, 0],
        vec![],
    ];
    assert!(has_diag_bl_tr(&f1));

    let f2 = vec![
        vec![1, 0, 0, 1, 0],
        vec![0, 0, 0, 1],
        vec![0, 0],
        vec![1, 1],
        vec![0],
        vec![1],
        vec![],
        vec![],
    ];
    assert!(!has_diag_bl_tr(&f2));
}

#[test]
fn check_diag_tl_br() {
    let f1 = vec![
        vec![1, 0, 0, 1, 0],
        vec![1, 1, 1, 1],
        vec![0, 0, 1],
        vec![1, 1, 0, 0],
        vec![1, 0, 1],
        vec![0, 1, 0, 1],
        vec![0, 1, 0, 0],
        vec![],
    ];
    assert!(has_diag_tl_br(&f1));

    let f2 = vec![
        vec![1, 0, 0, 1, 0],
        vec![0, 0, 0, 1],
        vec![0, 0],
        vec![1, 1],
        vec![0],
        vec![1],
        vec![],
        vec![],
    ];
    assert!(!has_diag_tl_br(&f2));
}
