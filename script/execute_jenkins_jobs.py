import argparse
import requests
import urllib.parse


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

    for index, parameter in enumerate(['0.55', '0.60', '0.65', '0.70', '0.75', '0.80', '0.85']):
        thread_id_offset = index * 16 % 1028
        numa_node = thread_id_offset // 64

        # Parameterized Build - Jenkins - Jenkins Wiki https://wiki.jenkins.io/display/JENKINS/Parameterized+Build

        # winning_percentage_for_win = fr'{parameter:0.6f}'
        # query = urllib.parse.urlencode({
        #     'EvalDir': fr'D:\hnoda\tanuki-wcsc29-2019-05-06\eval',
        #     'EvalSaveDir': fr'D:\hnoda\shogi\eval\tanuki-wcsc29-2019-05-06.winning_percentage_for_win={winning_percentage_for_win}',
        #     'targetdir': fr'D:\hnoda\shogi\training_data\suisho-wcsoc2020.shuffled',
        #     'eta': fr'1.0',
        #     'validation_set_file_name': fr'D:\hnoda\shogi\validation_data\suisho-wcsoc2020.shuffled\xaa',
        #     'ThreadIdOffset': fr'{index * 16 % 128}',
        #     'winning_percentage_for_win': fr'{winning_percentage_for_win}',
        #     'numa_node': fr'{numa_node}',
        #     'weight_by_progress': '1',
        # })

        # winning_percentage_for_win = fr'{parameter:0.6f}'
        # query = urllib.parse.urlencode({
        #     'eval1': fr'D:\hnoda\shogi\eval\tanuki-wcsc29-2019-05-06.winning_percentage_for_win={winning_percentage_for_win}\final',
        #     'eval2': fr'D:\hnoda\tanuki-denryu-tsec-1\eval',
        # })

        # draw_value1 = fr'{parameter}'
        # query = urllib.parse.urlencode({
        #     'eval1': fr'D:\hnoda\tanuki-denryu-tsec-1\eval',
        #     'eval2': fr'D:\hnoda\tanuki-denryu-tsec-1\eval',
        #     'draw_value1': fr'{draw_value1}',
        # })

        # query = urllib.parse.urlencode({
        #     'EvalDir': fr'D:\hnoda\tanuki-wcsc29.halfkp_vm_256x2-32-32',
        #     'Threads': fr'16',
        #     'EvalSaveDir': fr'D:\hnoda\shogi\eval\tanuki-wcsc29.halfkp_vm_256x2-32-32.suisho-wcsoc2020.{parameter}00M',
        #     'targetdir': fr'D:\hnoda\shogi\training_data\suisho-wcsoc2020.shuffled.{parameter}00M',
        #     'eta': fr'1.0',
        #     'weight_by_progress': '1',
        #     'validation_set_file_name': fr'D:\hnoda\shogi\validation_data\suisho-wcsoc2020.shuffled\xaa',
        #     'ThreadIdOffset': fr'{thread_id_offset}',
        #     'numa_node': fr'{numa_node}',
        #     'winning_percentage_for_win': fr'0.99',
        #     'YANEURAOU_EDITION': fr'YANEURAOU_ENGINE_NNUE_HALFKP_VM_256X2_32_32',
        # })

        # query = urllib.parse.urlencode({
        #     'eval1': fr'D:\hnoda\shogi\eval\tanuki-wcsc29.halfkp_vm_256x2-32-32.suisho-wcsoc2020.{parameter}00M\final',
        #     'eval2': fr'D:\hnoda\shogi\eval\tanuki-wcsc29.halfkp_vm_256x2-32-32.suisho-wcsoc2020\final',
        #     'YANEURAOU_EDITION1': fr'YANEURAOU_ENGINE_NNUE_HALFKP_VM_256X2_32_32',
        #     'YANEURAOU_EDITION2': fr'YANEURAOU_ENGINE_NNUE_HALFKP_VM_256X2_32_32',
        # })

        # query = urllib.parse.urlencode({
        #     'EvalDir': fr'D:\hnoda\tanuki-wcsc29.halfkp_vm_256x2-32-32',
        #     'Threads': fr'16',
        #     'EvalSaveDir': fr'D:\hnoda\shogi\eval\tanuki-wcsc29.halfkp_vm_256x2-32-32.suisho-wcsoc2020.weight_by_progress=0.winning_percentage_for_win={parameter}',
        #     'targetdir': fr'D:\hnoda\shogi\training_data\suisho-wcsoc2020.shuffled',
        #     'eta': fr'1.0',
        #     'weight_by_progress': '0',
        #     'validation_set_file_name': fr'D:\hnoda\shogi\validation_data\suisho-wcsoc2020.shuffled\xaa',
        #     'ThreadIdOffset': fr'{thread_id_offset}',
        #     'numa_node': fr'{numa_node}',
        #     'winning_percentage_for_win': fr'{parameter}',
        #     'YANEURAOU_EDITION': fr'YANEURAOU_ENGINE_NNUE_HALFKP_VM_256X2_32_32',
        # })

        query = urllib.parse.urlencode({
            'eval1': fr'D:\hnoda\shogi\eval\tanuki-wcsc29.halfkp_vm_256x2-32-32.suisho-wcsoc2020.weight_by_progress=0.winning_percentage_for_win={parameter}/final',
            'eval2': fr'D:\hnoda\shogi\eval\tanuki-wcsc29.halfkp_vm_256x2-32-32.suisho-wcsoc2020\final',
            'YANEURAOU_EDITION1': fr'YANEURAOU_ENGINE_NNUE_HALFKP_VM_256X2_32_32',
            'YANEURAOU_EDITION2': fr'YANEURAOU_ENGINE_NNUE_HALFKP_VM_256X2_32_32',
        })

        url = f'http://{args.host_name}:8080/job/{args.projet_name}/buildWithParameters?{query}'
        print(url)
        requests.post(url, auth=('hnoda', args.token))


if __name__ == '__main__':
    main()
