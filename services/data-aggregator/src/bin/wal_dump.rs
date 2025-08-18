//! Simple WAL dump utility to inspect raw WAL contents

use std::fs::File;
use std::io::Read;
use byteorder::{LittleEndian, ReadBytesExt};

fn main() -> std::io::Result<()> {
    let mut file = File::open("./data/wal/0000000001.wal")?;
    
    // Read header
    let magic = file.read_u32::<LittleEndian>()?;
    let version = file.read_u32::<LittleEndian>()?;
    let entries = file.read_u64::<LittleEndian>()?;
    
    println!("WAL Header:");
    println!("  Magic: 0x{:08X} ({})", magic, 
        if magic == 0x5351574C { "SQWL - valid" } else { "INVALID" });
    println!("  Version: {version}");
    println!("  Entries in header: {entries}");
    
    // Try to read actual entries
    let mut count = 0;
    let mut total_bytes = 16; // header size
    
    loop {
        // Try to read entry header
        match file.read_u32::<LittleEndian>() {
            Ok(length) => {
                match file.read_u32::<LittleEndian>() {
                    Ok(crc) => {
                        // Read data
                        let mut data = vec![0u8; length as usize];
                        match file.read_exact(&mut data) {
                            Ok(()) => {
                                count += 1;
                                total_bytes += 8 + u64::from(length);
                                println!("Entry {count}: {length} bytes, CRC: 0x{crc:08X}");
                            }
                            Err(e) => {
                                println!("Failed to read entry data: {e}");
                                break;
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
            Err(_) => break,
        }
        
        if count >= 10 {
            println!("... (showing first 10 entries)");
            break;
        }
    }
    
    println!("\nActual entries found: {count}");
    println!("Total bytes processed: {total_bytes}");
    
    Ok(())
}