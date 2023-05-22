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

    parameters = [100, 200, 300, 400, 500, 600, 700, 800, 900, 1000]
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

        # branch1 = parameter[0]
        # branch2 = parameter[1]
        # query = urllib.parse.urlencode({
        #     'eval1': fr'D:\hnoda\shogi\eval\regression.v5.33.add.Suishopsv-150m.eta2=0.001.min_progress=0.1\{}',
        #     'eval2': fr'D:\hnoda\shogi\eval\halfkp_1024x2-8-32.add.Suishopsv-150m.eta2=0.001.min_progress=0.1\20',
        #     'YANEURAOU_EDITION1': fr'YANEURAOU_ENGINE_NNUE_HALFKP_1024X2_8_32',
        #     'YANEURAOU_EDITION2': fr'YANEURAOU_ENGINE_NNUE_HALFKP_1024X2_8_32',
        #     'fv_scale1': fr'16',
        #     'fv_scale2': fr'16',
        #     'hash': fr'384',
        #     'num_games': fr'1000',
        #     'branch1': fr'{branch1}',
        #     'branch2': fr'{branch2}',
        # })

        branch1 = 'a0f9bb59acf42e3146c035442f3598a7945fc7c9'
        branch2 = 'a0f9bb59acf42e3146c035442f3598a7945fc7c9'
        query = urllib.parse.urlencode({
            'eval1': fr'D:\hnoda\shogi\eval\nnue-pytorch.2023-05-19\{parameter}',
            'eval2': fr'D:\hnoda\shogi\eval\regression.v5.33\final',
            'YANEURAOU_EDITION1': fr'YANEURAOU_ENGINE_NNUE',
            'YANEURAOU_EDITION2': fr'YANEURAOU_ENGINE_NNUE',
            'fv_scale1': fr'16',
            'fv_scale2': fr'16',
            'hash': fr'768',
            'num_games': fr'2000',
            'branch1': fr'{branch1}',
            'branch2': fr'{branch2}',
        })

        url = f'http://{args.host_name}:8080/job/{args.projet_name}/buildWithParameters?{query}'
        print(url)
        requests.post(url, auth=('hnoda', args.token))


if __name__ == '__main__':
    main()
