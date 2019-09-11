extern crate chrono;
extern crate rayon;

use std::collections::HashSet;
use std::error::Error;
use std::fs::File;
use std::io::ErrorKind;
use std::option::Iter;
use std::time::Instant;

use rayon::prelude::*;

use self::chrono::{DateTime, Local};

pub fn gen_cfg(warehouse_id_list: Vec<u32>, terminal_count: u32) -> () {
    let wh_id_set: HashSet<_> = warehouse_id_list.clone().drain(..).collect();
    let now: DateTime<Local> = Local::now();
    let ts = now.format("%Y%m%d_%H%M%S");
    wh_id_set.par_iter().for_each(|w| {
        File::create(format!("W{}_T{}_{}.cfg", w, 1, &ts)).unwrap_or_else(|error| {
            if error.kind() == ErrorKind::AlreadyExists {
                File::create("tmp.cfg").unwrap_or_else(|error| {
                    panic!("Error creating substitute file {:?}", error);
                })
            } else {
                panic!("Error creating configuration file {:?}", error);
            }
        });
    });
}