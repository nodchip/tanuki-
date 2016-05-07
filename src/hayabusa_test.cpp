#include <filesystem>
#include <fstream>
#include <gtest/gtest.h>
#include "hayabusa.hpp"

using namespace std;
using namespace std::tr2::sys;

static const path TEST_INPUT_CSA_DIRECTORY_PATH = "../src/testdata/csa";
static const path TEST_OUTPUT_TEACHER_DATA_FILE_PATH = "../test.teacherdata";
static const path TEST_INPUT_TEACHER_DATA_FILE_PATH = "../src/testdata/teacherdata/test.teacherdata";
static const path TEST_INPUT_SHOGIDOKORO_CSA_DIRECTORY_PATH = "../src/testdata/shogidokoro/csa";
static const path TEST_INPUT_SFEN_FILE_PATH = "../src/testdata/sfen/test.sfen";

class HayabusaTest : public testing::Test {
public:
  HayabusaTest() {}
  virtual ~HayabusaTest() {}
protected:
  virtual void SetUp() {
    remove_all(TEST_OUTPUT_TEACHER_DATA_FILE_PATH);
  }
  virtual void TearDown() {
    remove_all(TEST_OUTPUT_TEACHER_DATA_FILE_PATH);
  }
private:
};

//TEST_F(HayabusaTest, converSfenToTeacherData_evaluatesPositions) {
//  EXPECT_TRUE(hayabusa::converSfenToTeacherData(
//    TEST_INPUT_CSA_DIRECTORY_PATH,
//    TEST_OUTPUT_TEACHER_DATA_FILE_PATH,
//    3));
//
//  EXPECT_EQ(
//    sizeof(hayabusa::TeacherData) * 3,
//    file_size(TEST_OUTPUT_TEACHER_DATA_FILE_PATH));
//}

TEST_F(HayabusaTest, adjustWeights_applySteepestDescentMethod) {
  EXPECT_TRUE(hayabusa::adjustWeights(
    TEST_INPUT_TEACHER_DATA_FILE_PATH,
    3));
}

//TEST_F(HayabusaTest, addTeacherData_addToExistingFile) {
//  ASSERT_TRUE(hayabusa::createTeacherData(
//    TEST_INPUT_CSA_DIRECTORY_PATH,
//    TEST_OUTPUT_TEACHER_DATA_FILE_PATH,
//    3));
//  EXPECT_TRUE(hayabusa::addTeacherData(
//    TEST_INPUT_SHOGIDOKORO_CSA_DIRECTORY_PATH,
//    TEST_OUTPUT_TEACHER_DATA_FILE_PATH,
//    3));
//
//  EXPECT_EQ(
//    sizeof(hayabusa::TeacherData) * 6,
//    file_size(TEST_OUTPUT_TEACHER_DATA_FILE_PATH));
//}