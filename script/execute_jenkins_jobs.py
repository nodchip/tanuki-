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

    # parameters = ['0', '10', '20', '30', '40', '50', '60', '70', '80', '90', '100', '1000']
    # parameters = ['0']
    # parameters = ['10', '20', '30', '40', '50', '60', '70', '80', '90', '100', '1000']
    # parameters = ['200000000', '300000000', '400000000', '500000000']
    # parameters = ['10', '100', '1000', '10000']
    parameters = ['0.1', '0.2', '0.3', '0.4', '0.5', '0.6', '0.7', '0.8', '0.9']
    for index, parameter in enumerate(parameters):
        thread_id_offset = index * 16 % 128

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

        # query = urllib.parse.urlencode({
        #     'eval1': fr'D:\hnoda\shogi\eval\tanuki-wcsc29.halfkp_vm_256x2-32-32.suisho-wcsoc2020.weight_by_progress=0.winning_percentage_for_win={parameter}/final',
        #     'eval2': fr'D:\hnoda\shogi\eval\tanuki-wcsc29.halfkp_vm_256x2-32-32.suisho-wcsoc2020\final',
        #     'YANEURAOU_EDITION1': fr'YANEURAOU_ENGINE_NNUE_HALFKP_VM_256X2_32_32',
        #     'YANEURAOU_EDITION2': fr'YANEURAOU_ENGINE_NNUE_HALFKP_VM_256X2_32_32',
        # })

        # generator_start_position_max_play = parameter
        # query = urllib.parse.urlencode({
        #     'KifuDir': fr'D:\hnoda\shogi\training_data\suisho5.generator_start_position_max_play={generator_start_position_max_play}',
        #     'GeneratorNumPositions': fr'500000000',
        #     'GeneratorStartposFileName': fr'D:\hnoda\shogi\startpos.2021-12-25.3900.sfen',
        #     'loop': fr'1',
        #     'GeneratorStartPositionMaxPlay': fr'{generator_start_position_max_play}',
        #     'YANEURAOU_EDITION': fr'YANEURAOU_ENGINE_NNUE',
        # })

        # generator_start_position_max_play = parameter
        # query = urllib.parse.urlencode({
        #     'kifu_folder_name': fr'suisho5.generator_start_position_max_play={generator_start_position_max_play}',
        # })

        # generator_start_position_max_play = parameter
        # query = urllib.parse.urlencode({
        #     'EvalDir': fr'D:\hnoda\tanuki-wcsc29.halfkp_vm_256x2-32-32',
        #     'Threads': fr'16',
        #     'EvalSaveDir': fr'D:\hnoda\shogi\eval\suisho5.generator_start_position_max_play={generator_start_position_max_play}',
        #     'targetdir': fr'D:\hnoda\shogi\training_data\suisho5.generator_start_position_max_play={generator_start_position_max_play}.shuffled',
        #     'eta': fr'1.0',
        #     'weight_by_progress': '0',
        #     'validation_set_file_name': fr'D:\hnoda\shogi\validation_data\suisho5.generator_start_position_max_play={generator_start_position_max_play}.shuffled\xaa',
        #     'ThreadIdOffset': fr'{thread_id_offset}',
        #     'winning_percentage_for_win': fr'0.80',
        #     'YANEURAOU_EDITION': fr'YANEURAOU_ENGINE_NNUE_HALFKP_VM_256X2_32_32',
        # })

        # generator_start_position_max_play = parameter
        # query = urllib.parse.urlencode({
        #     'EvalDir': fr'D:\hnoda\tanuki-wcsc29.halfkp_vm_256x2-32-32',
        #     'Threads': fr'16',
        #     'EvalSaveDir': fr'D:\hnoda\shogi\eval\suisho5.generator_start_position_max_play={generator_start_position_max_play}',
        #     'targetdir': fr'D:\hnoda\shogi\training_data\suisho5.generator_start_position_max_play={generator_start_position_max_play}.shuffled',
        #     'eta': fr'1.0',
        #     'weight_by_progress': '0',
        #     'validation_set_file_name': fr'D:\hnoda\shogi\validation_data\suisho5.generator_start_position_max_play={generator_start_position_max_play}.shuffled\xaa',
        #     'ThreadIdOffset': fr'{thread_id_offset}',
        #     'winning_percentage_for_win': fr'0.80',
        #     'YANEURAOU_EDITION': fr'YANEURAOU_ENGINE_NNUE_HALFKP_VM_256X2_32_32',
        # })

        # generator_start_position_max_play = parameter
        # query = urllib.parse.urlencode({
        #     'eval1': fr'D:\hnoda\shogi\eval\suisho5.generator_start_position_max_play={generator_start_position_max_play}\final',
        #     'eval2': fr'D:\hnoda\shogi\eval\suisho5\final',
        #     'YANEURAOU_EDITION1': fr'YANEURAOU_ENGINE_NNUE_HALFKP_VM_256X2_32_32',
        #     'YANEURAOU_EDITION2': fr'YANEURAOU_ENGINE_NNUE_HALFKP_VM_256X2_32_32',
        # })

        # eval_save_interval = parameter
        # query = urllib.parse.urlencode({
        #     'EvalDir': fr'D:\hnoda\tanuki-wcsc29.halfkp_vm_256x2-32-32',
        #     'Threads': fr'16',
        #     'EvalSaveDir': fr'D:\hnoda\shogi\eval\suisho5.eval_save_interval={eval_save_interval}',
        #     'targetdir': fr'D:\hnoda\shogi\training_data\suisho5.shuffled',
        #     'eta': fr'1.0',
        #     'weight_by_progress': '0',
        #     'validation_set_file_name': fr'D:\hnoda\shogi\validation_data\suisho5.shuffled\xaa',
        #     'ThreadIdOffset': fr'{thread_id_offset}',
        #     'winning_percentage_for_win': fr'0.80',
        #     'YANEURAOU_EDITION': fr'YANEURAOU_ENGINE_NNUE_HALFKP_VM_256X2_32_32',
        #     'eval_save_interval': fr'{eval_save_interval}',
        # })

        # eval_save_interval = parameter
        # query = urllib.parse.urlencode({
        #     'eval1': fr'D:\hnoda\shogi\eval\suisho5.eval_save_interval={eval_save_interval}\final',
        #     'eval2': fr'D:\hnoda\shogi\eval\suisho5\final',
        #     'YANEURAOU_EDITION1': fr'YANEURAOU_ENGINE_NNUE_HALFKP_VM_256X2_32_32',
        #     'YANEURAOU_EDITION2': fr'YANEURAOU_ENGINE_NNUE_HALFKP_VM_256X2_32_32',
        # })

        # eta1_epoch = parameter
        # query = urllib.parse.urlencode({
        #     'EvalDir': fr'D:\hnoda\tanuki-wcsc29.halfkp_vm_256x2-32-32',
        #     'Threads': fr'16',
        #     'EvalSaveDir': fr'D:\hnoda\shogi\eval\suisho5.eta1_epoch={eta1_epoch}',
        #     'targetdir': fr'D:\hnoda\shogi\training_data\suisho5.shuffled',
        #     'eta1': fr'1e-8',
        #     'eta2': fr'1.0',
        #     'eta1_epoch': fr'{eta1_epoch}',
        #     'weight_by_progress': '0',
        #     'validation_set_file_name': fr'D:\hnoda\shogi\validation_data\suisho5.shuffled\xaa',
        #     'ThreadIdOffset': fr'{thread_id_offset}',
        #     'winning_percentage_for_win': fr'0.80',
        #     'YANEURAOU_EDITION': fr'YANEURAOU_ENGINE_NNUE_HALFKP_VM_256X2_32_32',
        # })

        # eta1_epoch = parameter
        # query = urllib.parse.urlencode({
        #     'eval1': fr'D:\hnoda\shogi\eval\suisho5.eta1_epoch={eta1_epoch}\final',
        #     'eval2': fr'D:\hnoda\shogi\eval\suisho5\final',
        #     'YANEURAOU_EDITION1': fr'YANEURAOU_ENGINE_NNUE_HALFKP_VM_256X2_32_32',
        #     'YANEURAOU_EDITION2': fr'YANEURAOU_ENGINE_NNUE_HALFKP_VM_256X2_32_32',
        # })

        # momentum = parameter
        # query = urllib.parse.urlencode({
        #     'EvalDir': fr'D:\hnoda\tanuki-wcsc29.halfkp_vm_256x2-32-32',
        #     'Threads': fr'16',
        #     'EvalSaveDir': fr'D:\hnoda\shogi\eval\suisho5.momentum={momentum}',
        #     'targetdir': fr'D:\hnoda\shogi\training_data\suisho5.shuffled',
        #     'weight_by_progress': '0',
        #     'validation_set_file_name': fr'D:\hnoda\shogi\validation_data\suisho5.shuffled\xaa',
        #     'ThreadIdOffset': fr'{thread_id_offset}',
        #     'winning_percentage_for_win': fr'0.80',
        #     'YANEURAOU_EDITION': fr'YANEURAOU_ENGINE_NNUE_HALFKP_VM_256X2_32_32',
        #     'nn_options': fr'momentum={momentum}',
        # })

        momentum = parameter
        query = urllib.parse.urlencode({
            'eval1': fr'D:\hnoda\shogi\eval\suisho5.momentum={momentum}\final',
            'eval2': fr'D:\hnoda\shogi\eval\suisho5\final',
            'YANEURAOU_EDITION1': fr'YANEURAOU_ENGINE_NNUE_HALFKP_VM_256X2_32_32',
            'YANEURAOU_EDITION2': fr'YANEURAOU_ENGINE_NNUE_HALFKP_VM_256X2_32_32',
        })

        url = f'http://{args.host_name}:8080/job/{args.projet_name}/buildWithParameters?{query}'
        print(url)
        requests.post(url, auth=('hnoda', args.token))


if __name__ == '__main__':
    main()
