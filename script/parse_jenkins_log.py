import argparse
import re
import urllib
import urllib.request

# PATTERN0 = re.compile(r'=([-0-9]+)?. db')
# PATTERN0 = re.compile(r'depth=(\d+)')
# PATTERN0 = re.compile(r'lambda=([0-9.]+)')
# PATTERN0 = re.compile(r'eval_limit=([0-9]+)')
# PATTERN0 = re.compile(r'winning_percentage_for_win=([0-9.]+)')
# PATTERN0 = re.compile(r'l2_regularization_parameter=([0-9.]+)')
# PATTERN0 = re.compile(r'nn_batch_size=([0-9]+)')
# PATTERN0 = re.compile(r'DrawValue=([-0-9]+)')
# PATTERN0 = re.compile(r'([0-9]{3}M)')
# PATTERN0 = re.compile(r'winning_percentage_for_win=([0-9.]+)')
# PATTERN0 = re.compile(r'generator_start_position_max_play=([0-9]+)')
# PATTERN0 = re.compile(r'regression\.(.+?)\\')
# PATTERN0 = re.compile(r'FVScale1=([0-9.]+)')
# PATTERN0 = re.compile(r'eta2=([0-9.]+)')
# PATTERN0 = re.compile(r'0\.001\\(\d+)')
PATTERN0 = re.compile(r'(Branch_v5\.33_[0-9]+)')
PATTERN1 = re.compile(
    r'勝ち\d+\([0-9.]+% R([-0-9.]+) \+-[0-9.]+\) 先手勝ち\d+\([0-9.]+%\) 後手勝ち\d+\([0-9.]+%\)')
# BUILD_NUMBERS = [15, 16, 17, 18, 19, 20, 21, 22, 24, 25, 26, 27, 28, 29]
# BUILD_NUMBERS = [45, 46, 47, 48, 49]
# BUILD_NUMBERS = range(123, 129 + 1)
BUILD_NUMBERS = range(172, 180 + 1)

def main():
    parser = argparse.ArgumentParser(description='Execute Jenkins Jobs')
    parser.add_argument('--token', type=str,
                        help='Jenkins API Token.', required=True)
    parser.add_argument('--host_name', type=str,
                        help='Host name ex) nighthawk', required=True)
    parser.add_argument('--projet_name', type=str,
                        help='Project name ex) generate_kifu.2021-08-29', required=True)
    parser.add_argument('--user_name', type=str,
                        help='User name ex) nodchip', required=True)
    args = parser.parse_args()

    for build_number in BUILD_NUMBERS:
        url = f'http://{args.host_name}:8080/job/{args.projet_name}/{build_number}/consoleText'
        with urllib.request.urlopen(url) as response:
            html = response.read().decode('cp932')

        print(html.split('\r\n\r\n')[-2])
        print()

    for build_number in BUILD_NUMBERS:
        url = f'http://{args.host_name}:8080/job/{args.projet_name}/{build_number}/consoleText'
        with urllib.request.urlopen(url) as response:
            html = response.read().decode('cp932')

        key = 0
        last_key = 0

        # for m in re.finditer(PATTERN0, html):
        #     key = last_key
        #     last_key = m.group(1)

        for m in re.finditer(PATTERN0, html):
            key = m.group(1)
            break

        matched = None
        for m in re.finditer(PATTERN1, html):
            matched = m
        if not matched:
            continue
        print(f'{key}\t{matched.group(1)}')


if __name__ == '__main__':
    main()
