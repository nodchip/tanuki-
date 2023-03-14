import argparse
import requests
import urllib.parse
import math


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

    threads = 16
    num_cores = 128
    num_concurrent_processes = num_cores // threads

    # parameters = ['V5.40_post', 'V5.40',]
    # old_versions = ['V4.86', 'V4.85', 'V4.83', 'V4.82_NNUE',]
    # parameters = [0.0, -1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0, -10.0, ]
    # parameters = [0.0]
    # parameters = [-1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0, -10.0, ]
    # parameters = [5, 15, 25, 30, 35]
    # parameters = [0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0]
    parameters = [
        'Branch_v5.33_20201128192325',
        'Branch_v5.33_20201128225947',
        'Branch_v5.33_20201130081917',
        'Branch_v5.33_20201130102017',
        'Branch_v5.33_20201201163629',
        'Branch_v5.33_20201201214136',
        'Branch_v5.33_20201201214447',
        'Branch_v5.33_20201202075042',
        'Branch_v5.33_20201202080706',
    ]
    for index, parameter in enumerate(parameters):
        thread_id_offset = index * threads % num_cores

        # Parameterized Build - Jenkins - Jenkins Wiki https://wiki.jenkins.io/display/JENKINS/Parameterized+Build

        # max_progress = parameter
        # query = urllib.parse.urlencode({
        #     'kifu_folder_name': fr'Suishopsv-150m',
        #     'shuffled_kifu_folder_name': fr'Suishopsv-150m.min_progress=0.1.max_progress={max_progress}',
        #     'shuffle_kifu_for_test': fr'false',
        #     'ShuffledMinProgress': fr'0.1',
        #     'ShuffledMaxProgress': fr'{max_progress}',
        # })

        # branch = parameter
        # query = urllib.parse.urlencode({
        #     'EvalDir': fr'D:\hnoda\tanuki-wcsc29-2019-05-06\eval',
        #     'Threads': fr'{threads}',
        #     'EvalSaveDir': fr'D:\hnoda\shogi\eval\{branch}',
        #     'targetdir': fr'D:\hnoda\shogi\training_data\suisho5.shuffled',
        #     'eta2': fr'1.0',
        #     'lambda': fr'0.5',
        #     'validation_set_file_name': fr'D:\hnoda\shogi\validation_data\suisho5.shuffled\xaa',
        #     'ThreadIdOffset': fr'{thread_id_offset}',
        #     'winning_percentage_for_win': fr'0.99',
        #     'YANEURAOU_EDITION': fr'YANEURAOU_ENGINE_NNUE',
        #     'BRANCH': fr'{branch}'
        # })

        branch = parameter
        query = urllib.parse.urlencode({
            'eval1': fr'D:\hnoda\shogi\eval\{branch}\final',
            'eval2': fr'D:\hnoda\shogi\eval\suisho5.halfkp_256x2-32-32.80G\final',
            'YANEURAOU_EDITION1': fr'YANEURAOU_ENGINE_NNUE',
            'YANEURAOU_EDITION2': fr'YANEURAOU_ENGINE_NNUE',
            'fv_scale1': fr'16',
            'fv_scale2': fr'16',
            'hash': fr'768',
        })

        url = f'http://{args.host_name}:8080/job/{args.projet_name}/buildWithParameters?{query}'
        print(url)
        requests.post(url, auth=('hnoda', args.token))


if __name__ == '__main__':
    main()
