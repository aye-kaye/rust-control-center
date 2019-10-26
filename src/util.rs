use std::str::FromStr;

#[derive(Clone, Debug)]
pub struct Error {
    pub err_msg: String,
}

impl ToString for Error {
    fn to_string(&self) -> String {
        self.err_msg.clone()
    }
}

pub fn parse_nums(s: &str) -> Result<Box<Vec<u32>>, Error> {
    if !s.is_empty() {
        if s.contains(",") {
            let id_vec: Result<Vec<u32>, Error> = s.split(",").map(try_parse_num).collect();
            return id_vec.map(|vec| Box::from(vec));
        } else if s.contains("..") {
            let split: Vec<&str> = s.split("..").collect();
            if split.len() < 2 {
                return Err(Error {
                    err_msg: format!("Range must have both ends defined '{}'", s),
                });
            } else {
                let range = (
                    try_parse_num::<u32>(split.get(0).unwrap()),
                    try_parse_num::<u32>(split.get(1).unwrap()),
                );
                if let (Ok(s), Ok(e)) = range {
                    return Ok(Box::from((s..e + 1).map(|num| num).collect::<Vec<u32>>()));
                } else {
                    return Err(Error {
                        err_msg: format!("Range boundaries have an incorrect format '{}'", s),
                    });
                }
            }
        } else {
            match try_parse_num(s) {
                Ok(id) => return Ok(Box::from(vec![id])),
                Err(err) => return Err(err),
            }
        }
    }

    Ok(Box::from(vec![]))
}

fn format_num_parse_error(s: &str) -> String {
    format!("Number has an incorrect format '{}'", s)
}

fn try_parse_num<F: FromStr>(s: &str) -> Result<F, Error> {
    match s.parse::<F>() {
        Ok(num) => Ok(num),
        Err(_) => Err(Error {
            err_msg: format_num_parse_error(s),
        }),
    }
}
