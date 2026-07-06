use std::{
    sync::mpsc::{Receiver, Sender},
    thread::{scope, JoinHandle},
    time::Duration,
};

use mihomoctl_core::Clash;
use crossterm::event::Event as CrossTermEvent;
use log::warn;
use rayon::prelude::*;

use crate::{
    interactive::{ControllerKind, Flags, InteractiveError},
    ui::{
        event::{Event, UpdateEvent},
        utils::{Interval, Pulse},
        Action, TuiOpt, TuiResult,
    },
};

pub type Job = JoinHandle<TuiResult<()>>;

pub fn servo(tx: Sender<Event>, rx: Receiver<Action>, opt: TuiOpt, flags: Flags) -> TuiResult<()> {
    let config = flags.get_config()?;
    let server = config
        .using_server()
        .ok_or(InteractiveError::ServerNotFound)?
        .to_owned();
    let controller_kind = server.kind;
    let clash = server.into_clash_with_timeout(Some(Duration::from_millis(flags.timeout)))?;
    clash.get_version()?;

    scope(|r| -> TuiResult<()> {
        let tx_clone = tx.clone();
        let handle1 = r.spawn(|| input_job(tx_clone));

        let tx_clone = tx.clone();
        let handle2 = r.spawn(|| traffic_job(tx_clone, &clash));

        let tx_clone = tx.clone();
        let handle3 = r.spawn(|| log_job(tx_clone, &clash));

        let handle4 = if controller_kind == ControllerKind::Mihomo {
            let tx_clone = tx.clone();
            Some(r.spawn(|| memory_job(tx_clone, &clash)))
        } else {
            None
        };

        let tx_clone = tx.clone();
        let handle5 = r.spawn(|| req_job(&opt, &flags, tx_clone, &clash));

        let handle6 = r.spawn(|| action_job(&opt, &flags, tx, rx, &clash, controller_kind));

        handle1.join().unwrap()?;
        handle2.join().unwrap()?;
        handle3.join().unwrap()?;
        if let Some(handle4) = handle4 {
            handle4.join().unwrap()?;
        }
        handle5.join().unwrap()?;
        handle6.join().unwrap()?;

        Ok(())
    })
}

fn input_job(tx: Sender<Event>) -> TuiResult<()> {
    loop {
        match crossterm::event::read() {
            Ok(CrossTermEvent::Key(event)) => tx.send(Event::from(event))?,
            Err(_) => {
                tx.send(Event::Quit)?;
                break;
            }
            _ => {}
        }
    }
    Ok(())
}

fn req_job(_opt: &TuiOpt, _flags: &Flags, tx: Sender<Event>, clash: &Clash) -> TuiResult<()> {
    let mut interval = Interval::every(Duration::from_millis(50));
    let mut connection_pulse = Pulse::new(20); // Every 1 s
    let mut proxies_pulse = Pulse::new(100); //   Every 5 s + 0 tick
    let mut rules_pulse = Pulse::new(101); //     Every 5 s + 1 tick
    let mut version_pulse = Pulse::new(102); //   Every 5 s + 2 tick
    let mut config_pulse = Pulse::new(103); //    Every 5 s + 3 tick

    send_update(&tx, "version", || clash.get_version(), UpdateEvent::Version)?;
    send_update(
        &tx,
        "connections",
        || clash.get_connections().map(Into::into),
        UpdateEvent::Connection,
    )?;
    send_update(&tx, "rules", || clash.get_rules(), UpdateEvent::Rules)?;
    send_update(&tx, "proxies", || clash.get_proxies(), UpdateEvent::Proxies)?;
    send_update(&tx, "configs", || clash.get_configs(), UpdateEvent::Config)?;

    loop {
        if version_pulse.tick() {
            send_update(&tx, "version", || clash.get_version(), UpdateEvent::Version)?;
        }
        if connection_pulse.tick() {
            send_update(
                &tx,
                "connections",
                || clash.get_connections().map(Into::into),
                UpdateEvent::Connection,
            )?;
        }
        if rules_pulse.tick() {
            send_update(&tx, "rules", || clash.get_rules(), UpdateEvent::Rules)?;
        }
        if proxies_pulse.tick() {
            send_update(&tx, "proxies", || clash.get_proxies(), UpdateEvent::Proxies)?;
        }
        if config_pulse.tick() {
            send_update(&tx, "configs", || clash.get_configs(), UpdateEvent::Config)?;
        }
        interval.tick();
    }
}

fn send_update<T, E, F, M>(
    tx: &Sender<Event>,
    label: &str,
    fetch: F,
    into_update: M,
) -> TuiResult<()>
where
    E: std::fmt::Display,
    F: FnOnce() -> Result<T, E>,
    M: FnOnce(T) -> UpdateEvent,
{
    match fetch() {
        Ok(value) => tx.send(Event::Update(into_update(value)))?,
        Err(error) => warn!("Failed to refresh {label}: {error}"),
    }
    Ok(())
}

fn traffic_job(tx: Sender<Event>, clash: &Clash) -> TuiResult<()> {
    let mut traffics = clash.get_traffic()?;
    loop {
        match traffics.next() {
            Some(Ok(traffic)) => tx.send(Event::Update(UpdateEvent::Traffic(traffic)))?,
            Some(Err(e)) => warn!("{:?}", e),
            None => warn!("No more traffic"),
        }
    }
}

fn log_job(tx: Sender<Event>, clash: &Clash) -> TuiResult<()> {
    loop {
        let mut logs = clash.get_log()?;
        match logs.next() {
            Some(Ok(log)) => tx.send(Event::Update(UpdateEvent::Log(log)))?,
            Some(Err(e)) => warn!("{:?}", e),
            None => warn!("No more traffic"),
        }
    }
}

fn memory_job(tx: Sender<Event>, clash: &Clash) -> TuiResult<()> {
    let mut memories = clash.get_memory()?;
    loop {
        match memories.next() {
            Some(Ok(memory)) => tx.send(Event::Update(UpdateEvent::Memory(memory)))?,
            Some(Err(e)) => warn!("{:?}", e),
            None => warn!("No more memory"),
        }
    }
}

fn action_job(
    _opt: &TuiOpt,
    flags: &Flags,
    tx: Sender<Event>,
    rx: Receiver<Action>,
    clash: &Clash,
    controller_kind: ControllerKind,
) -> TuiResult<()> {
    while let Ok(action) = rx.recv() {
        tx.send(Event::Action(action.clone()))?;
        match action {
            Action::TestLatency { proxies } => {
                let result = proxies
                    .par_iter()
                    .filter_map(|proxy| {
                        clash
                            .get_proxy_delay(proxy, flags.test_url.as_str(), flags.timeout)
                            .err()
                    })
                    .collect::<Vec<_>>();

                let count = result.len();

                if count != 0 {
                    warn!(
                        "   {}",
                        result
                            .into_iter()
                            .map(|x| x.to_string())
                            .collect::<Vec<_>>()
                            .join(" ")
                    );
                    warn!("({}) error(s) during test proxy delay", count);
                }

                tx.send(Event::Update(UpdateEvent::ProxyTestLatencyDone))?;
                tx.send(Event::Update(UpdateEvent::Proxies(clash.get_proxies()?)))?;
            }
            Action::ApplySelection { group, proxy } => {
                let _ = clash
                    .set_proxygroup_selected(&group, &proxy)
                    .map_err(|e| warn!("{:?}", e));
                tx.send(Event::Update(UpdateEvent::Proxies(clash.get_proxies()?)))?;
            }
            Action::InvokeApi { operation, params } => {
                let result = operation.invoke_for_kind(
                    &params,
                    clash,
                    controller_kind,
                    flags.test_url.as_str(),
                    flags.timeout,
                );
                tx.send(Event::Update(UpdateEvent::ApiResult {
                    operation,
                    result,
                }))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::channel;

    use mihomoctl_core::model::{Version, VersionPayload};

    use super::*;
    use crate::ui::{event::UpdateEvent, TuiError};

    fn version() -> Version {
        Version {
            premium: None,
            version: VersionPayload::Raw("test".to_owned()),
        }
    }

    #[test]
    fn send_update_emits_successful_refresh() {
        let (tx, rx) = channel();

        send_update(&tx, "version", || Ok::<_, TuiError>(version()), UpdateEvent::Version)
            .unwrap();

        assert!(matches!(
            rx.try_recv().unwrap(),
            Event::Update(UpdateEvent::Version(_))
        ));
    }

    #[test]
    fn send_update_keeps_refresh_job_alive_when_one_request_fails() {
        let (tx, rx) = channel();

        send_update(
            &tx,
            "version",
            || Err::<Version, _>(TuiError::TuiInternalErr),
            UpdateEvent::Version,
        )
        .unwrap();

        assert!(rx.try_recv().is_err());
    }
}
