use rand::Rng;
use std::cmp::Ordering;
use std::io::{self, Write};

fn main() {
    println!("Hello, world!");

    let correct = rand::rng().random_range(1..=100);

    loop {
        print!("Guess a number between 1 and 100: ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let Ok(guess) = input.trim().parse::<i32>() else {
            println!("Invalid.");
            continue;
        };

        match correct.cmp(&guess) {
            Ordering::Greater => println!("Guess higher."),
            Ordering::Less => println!("Guess lower."),
            Ordering::Equal => {
                println!("Correct!");
                break;
            }
        }
    }

    println!("Goodbye, world.");
}
