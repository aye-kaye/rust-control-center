# RUST control center
Config generator and report builder for terminal emulator for TPC-C.

## Run cli_gen example

### Generate mode

`./cli_gen generate -w 1..20 -t 10 -x 100`

Where 
 - `-w, --warehouse-id-list 1..20` list of warehouse IDs. Can be a single value, a comma separated list or a range (both ends are included) 
 - `-t, --terminal-count 10` number of terminals PER warehouse
 - `-x, --transaction-count 100` number of transactions per deck per terminal
 
 
### Report mode

`./cli_gen test-report -l *.log -b 0m -e 15m`

Where
 - `-l, --log-files-glob *.log` glob pattern for consuming log files with INTERNAL csv format
 - `-b, --steady-begin-offset 0m` begin of the measurement (steady) interval defined as a time offset from the latest `time_started` value throughout the log files provided. Accepts values in a human readable format, e.g. `1m` or `1h 15m`
 - `-e, --steady-length 2h 15m` length of the measurement (steady) interval. Accepts values in a human readable format, e.g. `1m` or `1h 15m`
 
### Log format

Formatted as csv, with following columns


**time\_started**|**type**|**running\_time**|**tx\_running\_time**|**think\_time\_ms**|**is\_rbk**
:-----:|:-----:|:-----:|:-----:|:-----:|:-----:
1570199082889|StockLevel|223|225|1500|false

