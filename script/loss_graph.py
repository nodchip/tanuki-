import collections
import datetime
import matplotlib.pyplot as plt
import numpy as np
import re
import urllib
import urllib.request
import time


Pattern = collections.namedtuple('Pattern', ['label', 'regex_pattern'])


URL = 'url'
LABEL = 'label'
HTML = 'html'
DATA_LIST = [
    {
        URL: 'http://hnoda-dt2:8080/job/learn.2022-03-15/53/consoleText',
        LABEL: 'suisho5.40G',
    },
    {
        URL: 'http://hnoda-dt2:8080/job/learn.2022-03-15/54/consoleText',
        LABEL: 'suisho5.80G',
    },
]
PATTERNS = [
    Pattern('learn_cross_entropy',
            r', ([\d]+) sfens, .+ , learn_cross_entropy = ([.\d]+) ,'),
    Pattern('test_cross_entropy',
            r', ([\d]+) sfens, .+ , test_cross_entropy = ([.\d]+) ,'),
    # Pattern('move_accuracy',
    #         r', ([\d]+) sfens, .+ , move accuracy = ([.\d]+)% , '),
    Pattern('eta', r', ([\d]+) sfens, .+, eta = ([.\d]+),'),
    Pattern('hirate_eval',
            r', ([\d]+) sfens, .+, hirate eval = ([.\d]+) ,'),
    Pattern('norm',
            r', ([\d]+) sfens, .+, norm = ([.\d+e]+) ,'),
]
MIN_SFENS = -1


def Show():
    start_time = time.time()

    for data in DATA_LIST:
        print(data[URL])
        with urllib.request.urlopen(data[URL]) as response:
            html = response.read().decode('cp932')
            data[HTML] = html

    for pattern_index, pattern in enumerate(PATTERNS):
        print(pattern.label)
        plt.figure(figsize=(1600.0/100.0, 1200.0/100.0))
        for data in DATA_LIST:
            xs = list()
            ys = list()
            for m in re.finditer(pattern.regex_pattern, data[HTML]):
                x = m.group(1)
                x = float(x)
                if x < MIN_SFENS:
                    continue
                xs.append(x)
                y = m.group(2)
                y = float(y)
                ys.append(y)
            if not xs:
                continue
            plt.plot(xs, ys, label=data[LABEL])

        # plt.ylim(0.195, 0.200)
        # plt.ylim(0.210, 0.215)
        plt.legend()
        plt.grid()
        plt.title(pattern.label)

        now = datetime.datetime.now()
        filename = f'image.{now.strftime("%Y-%m-%d-%H-%M-%S")}.{pattern_index}.{pattern.label}.png'
        plt.savefig(filename)

        plt.close()

    elapsed_time = time.time() - start_time
    print("elapsed_time:{0}".format(elapsed_time) + "[sec]")


def main():
    Show()


if __name__ == '__main__':
    main()
