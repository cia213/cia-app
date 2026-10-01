"""Regression tests independent of CUDA, model weights and bundled binaries."""
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch, MagicMock

ROOT = Path(__file__).resolve().parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'src-tauri/resources' / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


remap = load('time_remap')
worker = load('rife_worker')


class InputContractTests(unittest.TestCase):
    def test_invalid_factor_is_rejected_before_starting_a_process(self):
        with patch.object(remap.subprocess, 'Popen') as spawn:
            for factor in (0, 1, 2.5, 11, float('nan'), float('inf')):
                with self.subTest(factor=factor), self.assertRaises(ValueError):
                    remap.process_time_remap('source.mp4', factor=factor, output_path='out.mp4')
            spawn.assert_not_called()

    def test_audio_tempo_chain_stays_in_supported_range_and_preserves_speed(self):
        for speed in (0.1, 0.25, 0.5, 1, 2, 10):
            values = [float(item.split('=')[1]) for item in remap.build_atempo_filter(speed).split(',')]
            product = 1
            for value in values:
                self.assertGreaterEqual(value, 0.5)
                self.assertLessEqual(value, 2)
                product *= value
            self.assertAlmostEqual(product, speed)

    def test_subtitle_slowmo_keeps_text_and_scales_only_timeline_lines(self):
        source = '1\n00:00:01,125 --> 00:00:02,750\nCaption mentions 00:00:01,125\n'
        result = remap.retime_srt(source, 2)
        self.assertIn('00:00:02,250 --> 00:00:05,500', result)
        self.assertIn('Caption mentions 00:00:01,125', result)

    def test_probe_uses_rational_average_cadence_and_skips_cover_art(self):
        fixture = {'streams': [
            {'codec_type': 'video', 'disposition': {'attached_pic': 1}},
            {'codec_type': 'video', 'width': 720, 'height': 480,
             'r_frame_rate': '60/1', 'avg_frame_rate': '30000/1001', 'duration': '2',
             'sample_aspect_ratio': '8:9', 'nb_frames': '60'},
            {'codec_type': 'audio'},
        ]}
        with patch.object(remap, 'run_command', return_value=json.dumps(fixture)):
            info = remap.get_video_info('source', 'probe')
        self.assertEqual(info['rate'], '30000/1001')
        self.assertEqual(info['sar'], '8/9')
        self.assertEqual(info['width'], 720)
        self.assertEqual(info['video_index'], 1)
        self.assertTrue(info['has_audio'])

    def test_rotation_swaps_dimensions_and_inverts_pixel_aspect(self):
        fixture = {'streams': [{'codec_type': 'video', 'width': 720, 'height': 480,
                   'avg_frame_rate': '24/1', 'duration': '2', 'sample_aspect_ratio': '8:9',
                   'side_data_list': [{'rotation': 90}]}]}
        with patch.object(remap, 'run_command', return_value=json.dumps(fixture)):
            info = remap.get_video_info('source', 'probe')
        self.assertEqual((info['width'], info['height'], info['sar']), (480, 720, '9/8'))

    def test_invalid_video_and_missing_cadence_are_rejected(self):
        for fixture in ({'streams': [{'codec_type': 'audio'}]},
                        {'streams': [{'codec_type': 'video', 'width': 720, 'height': 480,
                                     'avg_frame_rate': '0/0', 'r_frame_rate': '0/0', 'duration': '2'}]}):
            with patch.object(remap, 'run_command', return_value=json.dumps(fixture)):
                with self.assertRaises(ValueError):
                    remap.get_video_info('source', 'probe')


class OwnershipTests(unittest.TestCase):
    def test_preexisting_destination_is_preserved_even_for_bracket_filename(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / 'source [v1].mp4'
            output = Path(directory) / 'out.mp4'
            source.write_bytes(b'source')
            output.write_bytes(b'existing output')
            with patch.object(remap.subprocess, 'Popen') as spawn:
                with self.assertRaises(FileExistsError):
                    remap.process_time_remap(source, factor=2, output_path=output)
                spawn.assert_not_called()
            self.assertEqual(source.read_bytes(), b'source')
            self.assertEqual(output.read_bytes(), b'existing output')

    def test_encoder_launch_failure_reaps_worker_and_cleans_owned_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            source = directory / 'source.mp4'
            source.write_bytes(b'source')
            model = directory / 'rife/train_log'
            model.mkdir(parents=True)
            for name in ('flownet.pkl', 'RIFE_HDv3.py', 'IFNet_HDv3.py'):
                (model / name).write_bytes(b'fixture')
            process = MagicMock()
            process.stdout = io.BytesIO()
            process.stderr = io.BytesIO()
            process.poll.return_value = None
            process.wait.return_value = 0
            info = {'rate': '24', 'fps': 24, 'width': 16, 'height': 16, 'duration': 1,
                    'transfer': 'bt709', 'sar': '1', 'has_audio': False, 'subtitles': [], 'video_index': 0}
            with patch.object(remap, 'get_video_info', return_value=info), \
                 patch.object(remap, 'ensure_tools_in_path'), \
                 patch.object(remap.subprocess, 'Popen', side_effect=[process, OSError('encoder missing')]):
                with self.assertRaisesRegex(OSError, 'encoder missing'):
                    remap.process_time_remap(source, factor=2, output_path=directory/'output.mp4',
                                             rife_dir=model.parent)
            process.kill.assert_called_once()
            process.wait.assert_called()
            self.assertFalse((directory/'output.mp4').exists())
            self.assertEqual(list(directory.glob('.cia-render-*')), [])
            self.assertEqual(source.read_bytes(), b'source')

    def test_truncated_decoder_frame_is_an_error_not_successful_eof(self):
        self.assertIsNone(worker.read_frame(io.BytesIO(), 6))
        self.assertEqual(worker.read_frame(io.BytesIO(b'123456'), 6), b'123456')
        with self.assertRaisesRegex(RuntimeError, 'incomplete RGB'):
            worker.read_frame(io.BytesIO(b'12345'), 6)


class ModelContractTests(unittest.TestCase):
    def test_plain_and_ddp_checkpoints_load_all_parameters_strictly(self):
        for checkpoint in ({'weight': 'tensor'}, {'module.weight': 'tensor'},
                           {'module.weight': 'tensor', 'module.teacher.weight': 'unused'}):
            with self.subTest(checkpoint=checkpoint):
                model, torch = MagicMock(), MagicMock()
                torch.load.return_value = checkpoint
                worker.load_model_weights(model, torch, 'model')
                model.flownet.load_state_dict.assert_called_once_with({'weight': 'tensor'}, strict=True)
                self.assertTrue(torch.load.call_args.kwargs['weights_only'])

    def test_incomplete_or_duplicate_checkpoint_is_rejected(self):
        model, torch = MagicMock(), MagicMock()
        model.flownet.load_state_dict.side_effect = RuntimeError('Missing key weight')
        torch.load.return_value = {'other': 'tensor'}
        with self.assertRaisesRegex(RuntimeError, 'Missing key'):
            worker.load_model_weights(model, torch, 'model')
        for checkpoint in ({}, {'weight': 'tensor', 'module.weight': 'tensor'}):
            torch.load.return_value = checkpoint
            with self.assertRaises(RuntimeError):
                worker.load_model_weights(model, torch, 'model')


if __name__ == '__main__':
    unittest.main()
