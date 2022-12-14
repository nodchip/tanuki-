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
    parameters = [10, 20, 30, 40, 50, 60, 70, ]
    for index, parameter in enumerate(parameters):
        thread_id_offset = index * threads % num_cores

        # Parameterized Build - Jenkins - Jenkins Wiki https://wiki.jenkins.io/display/JENKINS/Parameterized+Build

        # eta2 = 0.01 * math.pow(2.0, parameter)
        # query = urllib.parse.urlencode({
        #     'EvalDir': fr'D:\hnoda\shogi\eval\halfkp_1024x2-8-32\final',
        #     'Threads': fr'{threads}',
        #     'EvalSaveDir': fr'D:\hnoda\shogi\eval\halfkp_1024x2-8-32.add.nodes=2M.eta2={eta2}',
        #     'targetdir': fr'D:\hnoda\shogi\training_data\suisho5.nodes=2M.shuffled',
        #     'eta2': fr'{eta2}',
        #     'lambda': fr'0.0',
        #     'validation_set_file_name': fr'D:\hnoda\shogi\validation_data\suisho5.nodes=2M.shuffled\xaa',
        #     'ThreadIdOffset': fr'{thread_id_offset}',
        #     'winning_percentage_for_win': fr'0.99',
        #     'YANEURAOU_EDITION': fr'YANEURAOU_ENGINE_NNUE_HALFKP_1024X2_8_32',
        # })

        eta2 = 0.01 * math.pow(2.0, -10.0)
        save_index = parameter
        query = urllib.parse.urlencode({
            'eval1': fr'D:\hnoda\shogi\eval\halfkp_1024x2-8-32.add.nodes=2M.eta2={eta2}\{save_index}',
            'eval2': fr'D:\hnoda\shogi\eval\halfkp_1024x2-8-32\final',
            'YANEURAOU_EDITION1': fr'YANEURAOU_ENGINE_NNUE_HALFKP_1024X2_8_32',
            'YANEURAOU_EDITION2': fr'YANEURAOU_ENGINE_NNUE_HALFKP_1024X2_8_32',
            'fv_scale1': fr'16',
            'fv_scale2': fr'16',
            'hash': fr'512',
        })

        url = f'http://{args.host_name}:8080/job/{args.projet_name}/buildWithParameters?{query}'
        print(url)
        requests.post(url, auth=('hnoda', args.token))


if __name__ == '__main__':
    main()
