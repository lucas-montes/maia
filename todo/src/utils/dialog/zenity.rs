use std::process;

#[derive(Debug, Default)]
pub struct Zenity {
    icon: String,
    width: String,
    height: String,
    timeout: String,
}

impl Zenity {
    // https://doc.ubuntu-fr.org/zenity
    pub fn new() -> Zenity {
        Zenity {
            icon: String::from(""),
            width: String::from("21"),
            height: String::from("74"),
            timeout: String::from(""),
        }
    }

    fn execute(&self, args: Vec<&str>, title: &str) {
        let mut command = process::Command::new("zenity");
        command.arg("--window-icon");
        command.arg(self.icon.clone());
        command.arg("--width");
        command.arg(self.width.clone());
        command.arg("--height");
        command.arg(self.height.clone());
        command.arg("--timeout");
        command.arg(self.timeout.clone());
        command.arg("--title");
        command.arg(title);

        command.args(args);
        let _ = command.output();
    }

    pub fn show_input(&self, input: &str) {
        let args = vec!["--entry", "--text", &input];
        // if let Some(ref default) = input.default {
        //     args.push("--entry-text");
        //     args.push(default);
        // }
        self.execute(args, input);
    }

    pub fn show_message(&self, message: &str) {
        let args = vec!["--info", "--text", &message];
        self.execute(args, message);
    }

    pub fn show_password(&self, password: &str) {
        let args = vec!["--password"];
        self.execute(args, password);
    }

    pub fn show_question(&self, question: &str) {
        let args = vec!["--question", "--text", &question];
        self.execute(args, question);
    }
}
