import random
import time

def display_game_art():
    print("""
    🎮 NUMBER GUESSING GAME 🎮
    =======================
         _________
        |         |
        |  ?-?-?  |
        |_________|
    """)

def play_game():
    secret_number = random.randint(1, 100)
    attempts = 0
    max_attempts = 7

    display_game_art()
    print("I'm thinking of a number between 1 and 100!")
    print(f"Can you guess it in {max_attempts} tries?\n")

    while attempts < max_attempts:
        try:
            guess = int(input("Enter your guess: "))
            attempts += 1

            if guess < secret_number:
                print("🔼 Too low!")
                print(f"Attempts left: {max_attempts - attempts}")
            elif guess > secret_number:
                print("🔽 Too high!")
                print(f"Attempts left: {max_attempts - attempts}")
            else:
                print("\n🎉 CONGRATULATIONS! 🎉")
                print(f"You found the number in {attempts} attempts!")
                return

            print("--------------------")
            time.sleep(0.5)

        except ValueError:
            print("Please enter a valid number!")

    print("\n😢 Game Over!")
    print(f"The number was {secret_number}")

if __name__ == "__main__":
    while True:
        play_game()
        play_again = input("\nWant to play again? (yes/no): ").lower()
        if play_again != 'yes':
            print("\nThanks for playing! Goodbye! 👋")
            break

import time
from colorama import init, Fore, Style

# Initialize colorama
init()

def print_rainbow():
    colors = [Fore.RED, Fore.YELLOW, Fore.GREEN, Fore.BLUE, Fore.MAGENTA]
    message = "Hello, Rainbow World!"
    
    while True:
        for color in colors:
            print(color + message + Style.RESET_ALL)
            time.sleep(0.5)

# Note: You'll need to install colorama first:
# pip install colorama

print_rainbow()

pip install colorama