extern crate rayon;

use rayon::prelude::*;

use std::collections::HashSet;

pub fn gen_cfg(warehouse_id_list: Vec<u32>, terminal_count: u32) -> () {
    let wh_id_set: Vec<u32> = warehouse_id_list.clone().drain(..).collect::<Vec<u32>>();
    println!("{}", wh_id_set.par_iter().sum::<u32>());
}