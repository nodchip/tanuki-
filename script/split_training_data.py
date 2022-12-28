import argparse
import re
import urllib
import urllib.request
import sys
import os


def main():
    folder_path = sys.argv[1]
    print(f'{folder_path=}')

    input_file_path = os.path.join(folder_path, 'shuffled.bin')
    with open(input_file_path, 'rb') as input_file:
        validation_data_file_path = os.path.join(folder_path, 'validation_data.bin')
        with open(validation_data_file_path, 'wb') as validation_data:
            validation_data.write(input_file.read(40 * 1000 * 1000))
        
        training_data_file_path = os.path.join(folder_path, 'training_data.bin')
        with open(training_data_file_path, 'wb') as training_data:
            training_data.write(input_file.read())


if __name__ == '__main__':
    main()
