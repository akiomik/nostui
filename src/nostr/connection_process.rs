use color_eyre::eyre::{ErrReport, Result};
use nostr_sdk::prelude::*;

use crate::nostr::Connection;

pub struct ConnectionProcess {
    conn: Connection,
    req_tx: tokio::sync::mpsc::UnboundedSender<Event>,
    event_rx: tokio::sync::mpsc::UnboundedReceiver<Event>,
    terminate_rx: tokio::sync::mpsc::UnboundedReceiver<()>,
}

type NewConnectionProcess = (
    tokio::sync::mpsc::UnboundedReceiver<Event>,
    tokio::sync::mpsc::UnboundedSender<Event>,
    tokio::sync::mpsc::UnboundedSender<()>,
    ConnectionProcess,
);

impl ConnectionProcess {
    pub fn new(conn: Connection) -> Result<NewConnectionProcess> {
        let (req_tx, req_rx) = tokio::sync::mpsc::unbounded_channel();
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let (terminate_tx, terminate_rx) = tokio::sync::mpsc::unbounded_channel();

        Ok((
            req_rx,
            event_tx,
            terminate_tx,
            Self {
                conn,
                req_tx,
                event_rx,
                terminate_rx,
            },
        ))
    }

    pub fn run(mut self) {
        tokio::spawn(async move {
            let mut timeline = self.conn.subscribe_timeline().await?;

            loop {
                while let Ok(notification) = timeline.try_recv() {
                    if let RelayPoolNotification::Event { event, .. } = notification {
                        self.req_tx.send(event)?;
                    };
                }

                while let Ok(event) = self.event_rx.try_recv() {
                    self.conn.send(event).await?;
                }

                if self.terminate_rx.try_recv().is_ok() {
                    self.conn.close().await?;
                    break;
                }
            }

            Ok::<(), ErrReport>(())
        });
    }
}
