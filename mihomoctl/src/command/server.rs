use std::time::Duration;

use clap::Subcommand;
use log::{debug, info, warn};
use owo_colors::OwoColorize;
use requestty::{prompt, prompt_one, Answers, Question};
use terminal_size::{terminal_size, Height, Width};
use url::Url;

use crate::{
    interactive::{detect_controller_kind, ControllerKind, Flags, Server},
    Result,
};

// use crate::Result;

fn controller_kind_choices() -> [&'static str; 2] {
    [ControllerKind::Mihomo.as_str(), ControllerKind::Clash.as_str()]
}

fn controller_kind_default_index(detected: Option<ControllerKind>) -> usize {
    match detected.unwrap_or_default() {
        ControllerKind::Mihomo => 0,
        ControllerKind::Clash => 1,
    }
}

fn controller_kind_from_choice(choice: &str) -> ControllerKind {
    match choice {
        "clash" => ControllerKind::Clash,
        _ => ControllerKind::Mihomo,
    }
}

#[derive(Subcommand, Debug)]
#[clap(about = "Interacting with servers")]
pub enum ServerSubcommand {
    #[clap(alias = "a", about = "Add new server (alias a)")]
    Add,
    #[clap(about = "Select active server")]
    Use,
    #[clap(alias = "ls", about = "Show current active server")]
    List,
    #[clap(about = "Remove servers")]
    Del,
}

impl ServerSubcommand {
    pub fn handle(&self, flags: &Flags) -> Result<()> {
        let mut config = flags.get_config()?;

        match self {
            Self::Add => {
                let questions = [
                    Question::input("url")
                        .message("URL of Clash API")
                        .validate(|input, _| match Url::parse(input) {
                            Ok(_) => Ok(()),
                            Err(e) => Err(format!("Invalid URL: {}", e)),
                        })
                        .build(),
                    Question::password("secret")
                        .message("Secret of Clash API, default to None:")
                        .build(),
                ];
                let mut res = prompt(questions).expect("Error during prompt");
                debug!("{:#?}", res);
                let secret = match res.remove("secret").unwrap().try_into_string().unwrap() {
                    string if string == *"" => None,
                    secret => Some(secret),
                };

                let url_str = res.remove("url").unwrap().try_into_string().unwrap();
                let url = Url::parse(&url_str).unwrap();
                let detected = detect_controller_kind(
                    &url,
                    secret.as_deref(),
                    Some(Duration::from_millis(flags.timeout)),
                );
                if let Some(kind) = detected {
                    info!("Detected {} controller", kind);
                } else {
                    warn!("Unable to detect controller type from /version");
                };
                let message = match detected {
                    Some(kind) => format!("Confirm controller type (detected {kind})"),
                    None => "Select controller type".to_owned(),
                };
                let ans = prompt_one(
                    Question::select("kind")
                        .message(message)
                        .choices(controller_kind_choices())
                        .default(controller_kind_default_index(detected))
                        .build(),
                )?;
                let kind = controller_kind_from_choice(&ans.as_list_item().unwrap().text);

                let server = Server {
                    secret,
                    url: url.clone(),
                    kind,
                };

                info!("Adding {}", server);

                config.servers.push(server);
                debug!("{:#?}", config.servers);
                config.use_server(url)?;
                config.write()?;
            }
            Self::Use => {
                if config.servers.is_empty() {
                    warn!("No server configured yet. Use `mihomoctl server add` first.");
                    return Ok(());
                }
                let servers = config.servers.iter().map(|server| server.url.as_str());
                let ans = &prompt_one(
                    Question::select("server")
                        .message("Select active server to interact with")
                        .choices(servers)
                        .build(),
                )?;
                let ans_str = &ans.as_list_item().unwrap().text;
                config.use_server(Url::parse(ans_str).unwrap())?;
                config.write()?;
            }
            Self::List => {
                if config.servers.is_empty() {
                    warn!("No server configured yet. Use `mihomoctl server add` first.");
                    return Ok(());
                }
                let (Width(terminal_width), _) = terminal_size().unwrap_or((Width(70), Height(0)));
                let active = config.using_server();
                println!("\n{:-<1$}", "", terminal_width as usize);
                println!("{:<8}{:<10}{:<50}", "ACTIVE".green(), "KIND", "URL");
                println!("{:-<1$}", "", terminal_width as usize);
                for server in &config.servers {
                    let is_active = match active {
                        Some(active) => server == active,
                        _ => false,
                    };
                    println!(
                        "{:^8}{:<10}{:<50}",
                        if is_active { "→".green() } else { "".green() },
                        server.kind.as_str(),
                        server.url.as_str(),
                    )
                }
                println!("{:-<1$}\n", "", terminal_width as usize);
            }
            Self::Del => {
                if config.servers.is_empty() {
                    warn!("No server configured yet. Use `mihomoctl server add` first.");
                    return Ok(());
                }
                let servers = config.servers.iter().map(|server| server.url.as_str());
                let ans = &prompt([
                    Question::multi_select("server")
                        .message("Select server(s) to remove")
                        .choices(servers)
                        .build(),
                    Question::confirm("confirm")
                        .when(|prev: &Answers| {
                            prev.get("server")
                                .and_then(|x| x.as_list_items())
                                .map(|x| !x.is_empty())
                                .unwrap_or(false)
                        })
                        .default(true)
                        .message(|prev: &Answers| {
                            format!(
                                "Confirm to remove {} servers?",
                                prev.get("server").unwrap().as_list_items().unwrap().len()
                            )
                        })
                        .build(),
                ])?;
                match (
                    ans.get("server").and_then(|x| x.as_list_items()),
                    ans.get("confirm").and_then(|x| x.as_bool()),
                ) {
                    (None, _) => {
                        warn!("No servers given")
                    }
                    (Some(_), None | Some(false)) => {
                        warn!("Operation cancelled")
                    }
                    (Some(servers), Some(true)) => {
                        info!("Removing {} servers", servers.len());
                        let server_names =
                            servers.iter().map(|x| x.text.clone()).collect::<Vec<_>>();
                        config
                            .servers
                            .retain(|f| !server_names.contains(&f.url.to_string()))
                    }
                }
                debug!("{:#?}", config.servers);
                config.write()?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detected_controller_kind_is_default_confirmation_choice() {
        assert_eq!(
            controller_kind_default_index(Some(ControllerKind::Mihomo)),
            0
        );
        assert_eq!(
            controller_kind_default_index(Some(ControllerKind::Clash)),
            1
        );
    }

    #[test]
    fn unknown_controller_kind_defaults_to_mihomo_but_keeps_clash_choice() {
        assert_eq!(controller_kind_default_index(None), 0);
        assert_eq!(controller_kind_choices(), ["mihomo", "clash"]);
    }
}
