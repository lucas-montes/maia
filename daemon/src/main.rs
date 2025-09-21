use std::{fs, path::Path};

use serde::{Deserialize, Serialize};
use shared::protocol::MessageProtocol;
use tokio::{
    io::{self, AsyncWriteExt},
    net::UnixListener,
};

use health::service::{HealthDb, HealthService};
mod notifications;
mod database;
mod services;
mod cross_domain;

use database::Database;

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
enum Message {
    Goal(todo::goals::Commands),
    Task(todo::tasks::Commands),
    Ai(ai::cli::Commands),
    Finance(finance::cli::Commands),
    Health(health::cli::Commands),
    Knowledge(knowledge::cli::Commands),
}
impl MessageProtocol for Message {}

async fn handle_message(message: Message) -> String {
    // Initialize database (in a real implementation, this would be done once at startup)
    let db = match Database::new("maia.db").await {
        Ok(db) => db,
        Err(e) => {
            tracing::error!("Failed to initialize database: {}", e);
            return format!("Database error: {}", e);
        }
    };

    match message {
        Message::Health(health_cmd) => {
            handle_health_command(health_cmd, &db).await
        }
        Message::Goal(goal_cmd) => {
            tracing::info!("Processing goal command: {:?}", goal_cmd);
            "Goal command processed".to_string()
        }
        Message::Task(task_cmd) => {
            tracing::info!("Processing task command: {:?}", task_cmd);
            "Task command processed".to_string()
        }
        Message::Ai(ai_cmd) => {
            tracing::info!("Processing AI command: {:?}", ai_cmd);
            "AI command processed".to_string()
        }
        Message::Finance(finance_cmd) => {
            tracing::info!("Processing finance command: {:?}", finance_cmd);
            "Finance command processed".to_string()
        }
        Message::Knowledge(knowledge_cmd) => {
            tracing::info!("Processing knowledge command: {:?}", knowledge_cmd);
            "Knowledge command processed".to_string()
        }
    }
}

async fn handle_health_command(command: health::cli::Commands, db: &Database) -> String {
    use health::cli::Commands;
    let service = HealthService { db };
    match command {
        Commands::LogFood(log_food) => {
            match service.log_food(log_food).await {
                Ok(id) => format!("Food logged successfully with ID: {}", id),
                Err(e) => format!("Failed to log food: {}", e),
            }
        }
        Commands::AddFoodItem(add_food) => {
            match service.add_food_item(add_food).await {
                Ok(id) => format!("Food item added successfully with ID: {}", id),
                Err(e) => format!("Failed to add food item: {}", e),
            }
        }
        Commands::ListFood { date } => {
            match service.list_food_logs(date).await {
                Ok(logs) => format!("Food logs: {:?}", logs),
                Err(e) => format!("Failed to list food logs: {}", e),
            }
        }
        Commands::LogWeight(log_weight) => {
            match service.log_weight(log_weight).await {
                Ok(id) => format!("Weight logged successfully with ID: {}", id),
                Err(e) => format!("Failed to log weight: {}", e),
            }
        }
        Commands::WeightTrend { days } => {
            match service.get_weight_trend(days).await {
                Ok(trend) => format!("Weight trend: {:?}", trend),
                Err(e) => format!("Failed to get weight trend: {}", e),
            }
        }
        Commands::LogWorkout(log_workout) => {
            match service.log_workout(log_workout).await {
                Ok(id) => format!("Workout logged successfully with ID: {}", id),
                Err(e) => format!("Failed to log workout: {}", e),
            }
        }
        Commands::WorkoutProgress { exercise } => {
            match service.list_workout_progress(exercise).await {
                Ok(logs) => format!("Workout progress: {:?}", logs),
                Err(e) => format!("Failed to get workout progress: {}", e),
            }
        }
        Commands::AddGroceries(add_groceries) => {
            if let Some(receipt_image) = add_groceries.receipt_image {
                let date = add_groceries.date.unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string());
                match service.process_grocery_receipt(receipt_image, add_groceries.store, date).await {
                    Ok(response) => format!("Groceries processed: {:?}", response),
                    Err(e) => format!("Failed to process groceries: {}", e),
                }
            } else {
                "Receipt image is required for grocery processing".to_string()
            }
        }
        Commands::ListGroceries => {
            match service.list_groceries().await {
                Ok(groceries) => format!("Groceries: {:?}", groceries),
                Err(e) => format!("Failed to list groceries: {}", e),
            }
        }
    }
}

async fn listen_socket() {
    tracing::info!("Maia daemon started");
    let socket_path = "/tmp/maia.sock";
    let path = Path::new(socket_path);

    if path.exists() {
        fs::remove_file(path).expect("Failed to remove existing socket");
    }

    let listener = UnixListener::bind(socket_path).expect("Failed to open socket");
    tracing::info!("Socket bound to {}", socket_path);
    loop {
        tracing::info!("ready ");
        match listener.accept().await {
            Ok((mut stream, _addr)) => {
                loop {
                    //Wait for the socket to be readable
                    if let Err(err) = stream.readable().await {
                        tracing::error!(err=%err, "Failed to wait for readable: ");
                        continue;
                    };

                    let mut buf = Vec::with_capacity(4096);
                    match stream.try_read_buf(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            let response = match Message::from_bytes(&buf) {
                                Ok(message) => {
                                    handle_message(message).await
                                }
                                Err(err) => {
                                    tracing::error!(err=%err, "Failed to parse message");
                                    "error".into()
                                }
                            };

                            if let Err(err) = stream.write_all(response.as_bytes()).await {
                                tracing::error!(err=%err, "Stream is not ready");
                            };
                            if let Err(err) = stream.flush().await {
                                tracing::error!(err=%err, "Stream is not ready");
                            };
                        }
                        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                            continue;
                        }
                        Err(err) => {
                            tracing::error!(err=%err, "Stream is not ready");
                        }
                    }
                }
            }
            Err(err) => tracing::error!(err=%err, "Connection failed "),
        }
        tracing::info!("ready ");
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    tracing_subscriber::fmt::init();

   listen_socket().await;
}
