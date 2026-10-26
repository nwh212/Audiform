// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Meters inside one effect (gain reduction, output level), for effect
//! windows that draw a meter. Each effect slot gets a pair of atomics when
//! its processor is built (off the audio thread); the audio thread only
//! stores into them, and the window reads them by slot id.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use crate::session::model::Id;

/// Two readings: gain reduction in dB (0 or more) and output level in dBFS.
#[derive(Debug, Default)]
pub struct SlotMeter {
    gr: AtomicU32,
    out: AtomicU32,
}

impl SlotMeter {
    #[inline]
    pub fn store(&self, gr_db: f32, out_db: f32) {
        self.gr.store(gr_db.to_bits(), Ordering::Relaxed);
        self.out.store(out_db.to_bits(), Ordering::Relaxed);
    }
    pub fn read(&self) -> [f32; 2] {
        [
            f32::from_bits(self.gr.load(Ordering::Relaxed)),
            f32::from_bits(self.out.load(Ordering::Relaxed)),
        ]
    }
}

fn table() -> &'static Mutex<HashMap<Id, Arc<SlotMeter>>> {
    static T: OnceLock<Mutex<HashMap<Id, Arc<SlotMeter>>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The meter for a slot, made on first use. Call off the audio thread.
pub fn for_slot(id: Id) -> Arc<SlotMeter> {
    let mut t = table().lock().unwrap_or_else(|e| e.into_inner());
    t.entry(id)
        .or_insert_with(|| {
            let m = SlotMeter::default();
            m.store(0.0, -120.0);
            Arc::new(m)
        })
        .clone()
}

/// The latest readings for a slot, if it has a meter.
pub fn read(id: Id) -> Option<[f32; 2]> {
    table().lock().ok()?.get(&id).map(|m| m.read())
}
