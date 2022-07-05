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
    # {
    #     URL: 'http://hnoda-dt2:8080/job/learn.2022-05-06/7/consoleText',
    #     LABEL: 'suisho5.halfkp_256x2-32-32.preqsearch.evaluate',
    # },
    # {
    #     URL: 'http://hnoda-dt2:8080/job/learn.2022-05-20/1/consoleText',
    #     LABEL: 'V7.61',
    # },
    # {
    #     URL: 'http://hnoda-dt2:8080/job/learn.2022-05-20/2/consoleText',
    #     LABEL: 'v7.50-wcsc32',
    # },
    # {
    #     URL: 'http://hnoda-dt2:8080/job/learn.2022-05-20/3/consoleText',
    #     LABEL: 'v7.10',
    # },
    # {
    #     URL: 'http://hnoda-dt2:8080/job/learn.2022-05-20/4/consoleText',
    #     LABEL: 'v7.00',
    # },
    # {
    #     URL: 'http://hnoda-dt2:8080/job/learn.2022-05-20/5/consoleText',
    #     LABEL: 'v6.50',
    # },
    # {
    #     URL: 'http://hnoda-dt2:8080/job/learn.2022-05-20/6/consoleText',
    #     LABEL: 'v6.00',
    # },
    # {
    #     URL: 'http://hnoda-dt2:8080/job/learn.2022-05-20/8/consoleText',
    #     LABEL: 'V5.00',
    # },
    # {
    #     URL: 'http://hnoda-dt2:8080/job/learn.2022-05-20/10/consoleText',
    #     LABEL: 'V4.89',
    # },
    # {
    #     URL: 'http://hnoda-dt2:8080/job/learn.2022-05-20/11/consoleText',
    #     LABEL: 'V4.88',
    # },
    # {
    #     URL: 'http://hnoda-dt2:8080/job/learn.2022-05-20/12/consoleText',
    #     LABEL: 'V4.86',
    # },
    # {
    #     URL: 'http://hnoda-dt2:8080/job/learn.2022-05-20/13/consoleText',
    #     LABEL: 'V4.85',
    # },
    # {
    #     URL: 'http://hnoda-dt2:8080/job/learn.2022-05-20/14/consoleText',
    #     LABEL: 'V4.83',
    # },
    # {
    #     URL: 'http://hnoda-dt2:8080/job/learn.2022-05-20/15/consoleText',
    #     LABEL: 'V4.82_NNUE',
    # },
    # {
    #     URL: 'http://hnoda-dt2:8080/job/learn.2022-05-20/17/consoleText',
    #     LABEL: 'V5.40_post',
    # },
    # {
    #     URL: 'http://hnoda-dt2:8080/job/learn.2022-05-20/18/consoleText',
    #     LABEL: 'V5.40',
    # },
    {
        URL: 'http://hnoda-dt2:8080/job/learn.2022-05-20/7/consoleText',
        LABEL: 'v5.33',
    },
    {
        URL: 'http://hnoda-dt2:8080/job/learn.2022-05-20/20/consoleText',
        LABEL: 'v5.33.iteration=2',
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

        # plt.ylim(0.55, 0.56)
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
